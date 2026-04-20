use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};

use crate::model::entry::DataEntry;
use crate::serial::config::SerialConfig;

/// 送受信スレッドを起動する。
///
/// - `data_sender` に受信バイト (`DataEntry::Byte`) と送信バイト (`DataEntry::Sent`) を流す
/// - `send_rx` から送信要求 (`Vec<u8>`) を受け取る
/// - `stop` シグナルで停止
pub fn spawn_worker(
    config: &SerialConfig,
    idle_threshold: Duration,
    data_sender: Sender<DataEntry>,
    send_rx: Receiver<Vec<u8>>,
    stop: Receiver<()>,
) -> Result<std::thread::JoinHandle<()>, serialport::Error> {
    let byte_duration = config.byte_duration();

    #[cfg(target_os = "windows")]
    {
        // メインスレッドでポートを開いてエラーを呼び出し元に返す
        let port_handle = win32_worker::open_and_configure(config).map_err(|e| {
            serialport::Error::new(serialport::ErrorKind::Io(e.kind()), e.to_string())
        })?;
        let handle = std::thread::spawn(move || {
            if let Err(e) = win32_worker::run_with_handle(
                port_handle,
                idle_threshold,
                byte_duration,
                data_sender,
                send_rx,
                stop,
            ) {
                tracing::error!(error = %e, "送受信エラー");
            }
        });
        Ok(handle)
    }

    #[cfg(not(target_os = "windows"))]
    {
        let port = serialport::new(&config.port_name, config.baud_rate)
            .data_bits(match config.data_bits {
                7 => serialport::DataBits::Seven,
                _ => serialport::DataBits::Eight,
            })
            .parity(config.parity.to_serialport())
            .stop_bits(config.stop_bits.to_serialport())
            .timeout(Duration::from_millis(1))
            .open()?;

        let handle = std::thread::spawn(move || {
            fallback_worker_loop(
                port,
                idle_threshold,
                byte_duration,
                data_sender,
                send_rx,
                stop,
            );
        });
        Ok(handle)
    }
}

/// バッチ内のバイトを処理（タイムスタンプ補間付き）
fn process_bytes(
    data: &[u8],
    now: Instant,
    byte_duration: Duration,
    idle_threshold: Duration,
    last_byte_time: &mut Option<Instant>,
    sender: &Sender<DataEntry>,
) {
    let n = data.len();
    for (i, &byte) in data.iter().enumerate() {
        let offset = byte_duration * (n - 1 - i) as u32;
        let ts = now - offset;

        if let Some(last) = *last_byte_time
            && let Some(elapsed) = ts.checked_duration_since(last)
        {
            // UARTは全ビット受信後にバイト確定するため、
            // 測定値に先頭バイトの送信時間が含まれる → 補正
            let corrected = elapsed.saturating_sub(byte_duration);
            if corrected >= idle_threshold {
                let ms = corrected.as_secs_f64() * 1000.0;
                if sender.send(DataEntry::Idle(ms)).is_err() {
                    return;
                }
            }
        }
        if sender.send(DataEntry::Byte(byte, ts)).is_err() {
            return;
        }
        *last_byte_time = Some(ts);
    }
}

/// 送信バイト列をモニタに反映する (送信成功時に呼ぶ)。
///
/// 送信前に前回バイト (受信/送信問わず) との間隔を見て IDLE しきい値を超えていれば
/// `DataEntry::Idle` を先に挿入する。これによりライブ IDLE カウンタが送信エントリの
/// 後ろに追い越される (送信が IDLE の前に現れる) 現象を防ぐ。
fn emit_sent(
    data: &[u8],
    sender: &Sender<DataEntry>,
    last_byte_time: &mut Option<Instant>,
    byte_duration: Duration,
    idle_threshold: Duration,
) {
    if data.is_empty() {
        return;
    }
    let now = Instant::now();

    if let Some(last) = *last_byte_time
        && let Some(elapsed) = now.checked_duration_since(last)
    {
        let corrected = elapsed.saturating_sub(byte_duration);
        if corrected >= idle_threshold {
            let ms = corrected.as_secs_f64() * 1000.0;
            if sender.send(DataEntry::Idle(ms)).is_err() {
                return;
            }
        }
    }

    for &b in data {
        if sender.send(DataEntry::Sent(b, now)).is_err() {
            return;
        }
    }
    *last_byte_time = Some(now);
}

// ========== Windows: WaitCommEvent イベント駆動方式 ==========
#[cfg(target_os = "windows")]
mod win32_worker {
    use super::*;
    use crate::serial::config::{ParitySetting, StopBitsSetting};
    use std::collections::VecDeque;
    use std::io;
    use std::mem::{self, MaybeUninit};
    use std::ptr;
    use std::sync::{Arc, Mutex};
    use windows_sys::Win32::Devices::Communication::*;
    use windows_sys::Win32::Foundation::*;
    use windows_sys::Win32::Storage::FileSystem::*;
    use windows_sys::Win32::System::IO::*;
    use windows_sys::Win32::System::Threading::*;

    fn init_thread() {
        unsafe extern "system" {
            fn timeBeginPeriod(uPeriod: u32) -> u32;
        }
        unsafe {
            SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST);
            timeBeginPeriod(1);
        }
    }

    fn cleanup_thread() {
        unsafe extern "system" {
            fn timeEndPeriod(uPeriod: u32) -> u32;
        }
        unsafe {
            timeEndPeriod(1);
        }
    }

    fn open_overlapped(port_name: &str) -> io::Result<HANDLE> {
        let mut name = Vec::<u16>::with_capacity(4 + port_name.len() + 1);
        if !port_name.starts_with('\\') {
            name.extend(r"\\.\".encode_utf16());
        }
        name.extend(port_name.encode_utf16());
        name.push(0);

        let handle = unsafe {
            CreateFileW(
                name.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                ptr::null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OVERLAPPED,
                0 as HANDLE,
            )
        };

        if handle == INVALID_HANDLE_VALUE {
            Err(io::Error::last_os_error())
        } else {
            Ok(handle)
        }
    }

    fn configure_port(handle: HANDLE, config: &SerialConfig) -> io::Result<()> {
        unsafe {
            if SetupComm(handle, 4096, 4096) == 0 {
                return Err(io::Error::last_os_error());
            }

            let mut dcb: DCB = mem::zeroed();
            dcb.DCBlength = mem::size_of::<DCB>() as u32;
            if GetCommState(handle, &mut dcb) == 0 {
                return Err(io::Error::last_os_error());
            }

            dcb.BaudRate = config.baud_rate;
            dcb.ByteSize = config.data_bits;
            dcb.Parity = match config.parity {
                ParitySetting::None => NOPARITY,
                ParitySetting::Odd => ODDPARITY,
                ParitySetting::Even => EVENPARITY,
            };
            dcb.StopBits = match config.stop_bits {
                StopBitsSetting::One => ONESTOPBIT,
                StopBitsSetting::Two => TWOSTOPBITS,
            };
            // fBinary=1, フロー制御すべて無効
            dcb._bitfield = 0x0001;

            if SetCommState(handle, &dcb) == 0 {
                return Err(io::Error::last_os_error());
            }

            let timeouts = COMMTIMEOUTS {
                ReadIntervalTimeout: u32::MAX,
                ReadTotalTimeoutMultiplier: 0,
                ReadTotalTimeoutConstant: 0,
                WriteTotalTimeoutMultiplier: 0,
                WriteTotalTimeoutConstant: 0,
            };
            if SetCommTimeouts(handle, &timeouts) == 0 {
                return Err(io::Error::last_os_error());
            }

            PurgeComm(handle, PURGE_RXCLEAR | PURGE_TXCLEAR);
            Ok(())
        }
    }

    fn create_event() -> io::Result<HANDLE> {
        let h = unsafe { CreateEventW(ptr::null(), TRUE, FALSE, ptr::null()) };
        if h == 0 as HANDLE {
            Err(io::Error::last_os_error())
        } else {
            Ok(h)
        }
    }

    /// RAII guard for Win32 HANDLEs
    struct HandleGuard(HANDLE);
    impl Drop for HandleGuard {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.0) };
        }
    }

    /// メインスレッドでポートを開いて設定する（エラーを呼び出し元に返すため）
    pub fn open_and_configure(config: &SerialConfig) -> io::Result<isize> {
        let handle = open_overlapped(&config.port_name)?;
        if let Err(e) = configure_port(handle, config) {
            unsafe { CloseHandle(handle) };
            return Err(e);
        }
        Ok(handle)
    }

    /// ポートへ同期的に送信 (overlapped I/O で完了待ち)
    fn write_sync(handle: HANDLE, data: &[u8], write_event: HANDLE) -> io::Result<()> {
        if data.is_empty() {
            return Ok(());
        }
        let mut overlapped: OVERLAPPED = unsafe { mem::zeroed() };
        overlapped.hEvent = write_event;
        let mut written: u32 = 0;
        let ok = unsafe {
            WriteFile(
                handle,
                data.as_ptr(),
                data.len() as u32,
                &mut written,
                &mut overlapped,
            )
        };
        if ok == 0 {
            let err = unsafe { GetLastError() };
            if err != ERROR_IO_PENDING {
                return Err(io::Error::from_raw_os_error(err as i32));
            }
            // 完了待ち
            let ok2 = unsafe { GetOverlappedResult(handle, &overlapped, &mut written, TRUE) };
            if ok2 == 0 {
                return Err(io::Error::last_os_error());
            }
        }
        if (written as usize) != data.len() {
            return Err(io::Error::other(format!(
                "WriteFile: {} / {} bytes written",
                written,
                data.len()
            )));
        }
        Ok(())
    }

    /// 事前にオープン済みのハンドルで送受信ループを実行
    pub fn run_with_handle(
        handle: isize,
        idle_threshold: Duration,
        byte_duration: Duration,
        data_sender: Sender<DataEntry>,
        send_rx: Receiver<Vec<u8>>,
        stop: Receiver<()>,
    ) -> io::Result<()> {
        init_thread();

        let _handle_guard = HandleGuard(handle);

        unsafe {
            if SetCommMask(handle, EV_RXCHAR) == 0 {
                return Err(io::Error::last_os_error());
            }
        }

        let comm_event = create_event()?;
        let _comm_guard = HandleGuard(comm_event);
        let read_event = create_event()?;
        let _read_guard = HandleGuard(read_event);
        let write_event = create_event()?;
        let _write_guard = HandleGuard(write_event);
        // 停止用イベント: WaitForMultipleObjectsで使用
        let stop_event = create_event()?;
        let _stop_guard = HandleGuard(stop_event);
        // 送信要求通知イベント
        let send_event = create_event()?;
        let _send_guard = HandleGuard(send_event);

        let mut buf = [0u8; 256];
        let mut last_byte_time: Option<Instant> = None;
        let wait_handles = [comm_event, stop_event, send_event];

        // HANDLE は !Send なので Send を約束する newtype で包んで forwarder に渡す
        struct SendHandle(HANDLE);
        unsafe impl Send for SendHandle {}
        let stop_event_send = SendHandle(stop_event);
        let send_event_send = SendHandle(send_event);

        // forwarder が send_rx から取り出した電文を worker が取り出す中継キュー
        let pending: Arc<Mutex<VecDeque<Vec<u8>>>> = Arc::new(Mutex::new(VecDeque::new()));
        let pending_forwarder = Arc::clone(&pending);

        // 受信ループは WaitForMultipleObjects(INFINITE) でブロックするため、
        // stop / send チャネルを監視して対応するイベントを起こす forwarder を使う。
        // shutdown_tx は受信ループがエラー終了した際に forwarder を起こす経路。
        let (shutdown_tx, shutdown_rx) = crossbeam_channel::bounded::<()>(1);
        let forwarder = std::thread::spawn(move || {
            let SendHandle(stop_h) = stop_event_send;
            let SendHandle(send_h) = send_event_send;
            loop {
                crossbeam_channel::select! {
                    recv(stop) -> _ => {
                        unsafe { SetEvent(stop_h); }
                        break;
                    }
                    recv(shutdown_rx) -> _ => break,
                    recv(send_rx) -> payload => {
                        match payload {
                            Ok(bytes) => {
                                if let Ok(mut q) = pending_forwarder.lock() {
                                    q.push_back(bytes);
                                }
                                unsafe { SetEvent(send_h); }
                            }
                            // 送信チャネルが閉じた場合は forwarder を終了
                            // (本体ループは comm_event / stop_event で回るので問題ない)
                            Err(_) => break,
                        }
                    }
                }
            }
        });

        let mut comm_event_pending = false;
        loop {
            if !comm_event_pending {
                let mut event_mask: u32 = 0;
                let mut overlapped: OVERLAPPED = unsafe { mem::zeroed() };
                overlapped.hEvent = comm_event;

                let ret = unsafe { WaitCommEvent(handle, &mut event_mask, &mut overlapped) };
                if ret == 0 {
                    let err = unsafe { GetLastError() };
                    if err != ERROR_IO_PENDING {
                        break;
                    }
                }
                comm_event_pending = true;
            }

            // comm_event / stop_event / send_event のいずれかを待機
            let wait = unsafe { WaitForMultipleObjects(3, wait_handles.as_ptr(), FALSE, INFINITE) };
            if wait == WAIT_OBJECT_0 {
                comm_event_pending = false;
                // バイト到着 — 即座にタイムスタンプ取得
                let now = Instant::now();
                unsafe { ResetEvent(comm_event) };

                let mut errors: u32 = 0;
                let mut comstat = MaybeUninit::<COMSTAT>::uninit();
                unsafe { ClearCommError(handle, &mut errors, comstat.as_mut_ptr()) };

                // 通信エラーをチャネルに送信
                if errors & (CE_FRAME | CE_OVERRUN | CE_RXPARITY) != 0 {
                    let _ = data_sender.send(DataEntry::Error);
                }

                let available = unsafe { comstat.assume_init().cbInQue };

                if available > 0 {
                    let to_read = (available as usize).min(buf.len());
                    let mut read_overlapped: OVERLAPPED = unsafe { mem::zeroed() };
                    read_overlapped.hEvent = read_event;
                    let mut bytes_read: u32 = 0;

                    let read_ret = unsafe {
                        ReadFile(
                            handle,
                            buf.as_mut_ptr().cast(),
                            to_read as u32,
                            &mut bytes_read,
                            &mut read_overlapped,
                        )
                    };

                    if read_ret == 0 && unsafe { GetLastError() } == ERROR_IO_PENDING {
                        let ok = unsafe {
                            GetOverlappedResult(handle, &read_overlapped, &mut bytes_read, TRUE)
                        };
                        if ok == 0 {
                            break;
                        }
                    }

                    if bytes_read > 0 {
                        process_bytes(
                            &buf[..bytes_read as usize],
                            now,
                            byte_duration,
                            idle_threshold,
                            &mut last_byte_time,
                            &data_sender,
                        );
                    }
                }
            } else if wait == WAIT_OBJECT_0 + 2 {
                // 送信要求 — キューを drain して順に WriteFile
                unsafe { ResetEvent(send_event) };
                let to_send: Vec<Vec<u8>> = {
                    match pending.lock() {
                        Ok(mut q) => q.drain(..).collect(),
                        Err(_) => Vec::new(),
                    }
                };
                for payload in to_send {
                    match write_sync(handle, &payload, write_event) {
                        Ok(()) => emit_sent(
                            &payload,
                            &data_sender,
                            &mut last_byte_time,
                            byte_duration,
                            idle_threshold,
                        ),
                        Err(e) => {
                            tracing::warn!(error = %e, "送信失敗");
                        }
                    }
                }
            } else {
                // stop_event またはエラー
                unsafe { CancelIo(handle) };
                break;
            }
        }

        // stop_event を閉じる前に forwarder を終了させる (use-after-close 回避)
        let _ = shutdown_tx.send(());
        let _ = forwarder.join();
        cleanup_thread();
        Ok(())
    }
}

// ========== 非Windows用フォールバック ==========
#[cfg(not(target_os = "windows"))]
fn fallback_worker_loop(
    mut port: Box<dyn serialport::SerialPort>,
    idle_threshold: Duration,
    byte_duration: Duration,
    sender: Sender<DataEntry>,
    send_rx: Receiver<Vec<u8>>,
    stop: Receiver<()>,
) {
    let mut buf = [0u8; 256];
    let mut last_byte_time: Option<Instant> = None;

    loop {
        if stop.try_recv().is_ok() {
            break;
        }
        // 送信要求をすべて処理
        while let Ok(payload) = send_rx.try_recv() {
            match port.write_all(&payload) {
                Ok(()) => emit_sent(
                    &payload,
                    &sender,
                    &mut last_byte_time,
                    byte_duration,
                    idle_threshold,
                ),
                Err(e) => {
                    tracing::warn!(error = %e, "送信失敗");
                }
            }
        }
        match port.read(&mut buf) {
            Ok(n) if n > 0 => {
                let now = Instant::now();
                process_bytes(
                    &buf[..n],
                    now,
                    byte_duration,
                    idle_threshold,
                    &mut last_byte_time,
                    &sender,
                );
            }
            Ok(_) => {}
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(_) => break,
        }
    }
}
