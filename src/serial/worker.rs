use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};

use crate::model::entry::DataEntry;
use crate::serial::config::SerialConfig;

/// 受信スレッドを起動する
pub fn spawn_receiver(
    config: &SerialConfig,
    idle_threshold: Duration,
    sender: Sender<DataEntry>,
    stop: Receiver<()>,
) -> Result<std::thread::JoinHandle<()>, serialport::Error> {
    let byte_duration = config.byte_duration();

    #[cfg(target_os = "windows")]
    {
        let config = config.clone();
        let handle = std::thread::spawn(move || {
            if let Err(e) = win32_receiver::run(&config, idle_threshold, byte_duration, sender, stop)
            {
                eprintln!("受信エラー: {}", e);
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
            fallback_receiver_loop(port, idle_threshold, byte_duration, sender, stop);
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

        if let Some(last) = *last_byte_time {
            if let Some(elapsed) = ts.checked_duration_since(last) {
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
        }
        if sender.send(DataEntry::Byte(byte, ts)).is_err() {
            return;
        }
        *last_byte_time = Some(ts);
    }
}

// ========== Windows: WaitCommEvent イベント駆動方式 ==========
#[cfg(target_os = "windows")]
mod win32_receiver {
    use super::*;
    use crate::serial::config::{ParitySetting, StopBitsSetting};
    use std::io;
    use std::mem::{self, MaybeUninit};
    use std::ptr;
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
            SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST as i32);
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
                ParitySetting::None => NOPARITY as u8,
                ParitySetting::Odd => ODDPARITY as u8,
                ParitySetting::Even => EVENPARITY as u8,
            };
            dcb.StopBits = match config.stop_bits {
                StopBitsSetting::One => ONESTOPBIT as u8,
                StopBitsSetting::Two => TWOSTOPBITS as u8,
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

    pub fn run(
        config: &SerialConfig,
        idle_threshold: Duration,
        byte_duration: Duration,
        sender: Sender<DataEntry>,
        stop: Receiver<()>,
    ) -> io::Result<()> {
        init_thread();

        let handle = open_overlapped(&config.port_name)?;
        let _handle_guard = HandleGuard(handle);

        configure_port(handle, config)?;

        unsafe {
            if SetCommMask(handle, EV_RXCHAR) == 0 {
                return Err(io::Error::last_os_error());
            }
        }

        let comm_event = create_event()?;
        let _comm_guard = HandleGuard(comm_event);
        let read_event = create_event()?;
        let _read_guard = HandleGuard(read_event);
        // 停止用イベント: WaitForMultipleObjectsで使用
        let stop_event = create_event()?;
        let _stop_guard = HandleGuard(stop_event);

        let mut buf = [0u8; 256];
        let mut last_byte_time: Option<Instant> = None;
        let wait_handles = [comm_event, stop_event];

        loop {
            if stop.try_recv().is_ok() {
                unsafe { SetEvent(stop_event) };
                break;
            }

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

            // comm_event または stop_event を待機
            let wait = unsafe { WaitForMultipleObjects(2, wait_handles.as_ptr(), FALSE, INFINITE) };
            if wait == WAIT_OBJECT_0 {
                // バイト到着 — 即座にタイムスタンプ取得
                let now = Instant::now();
                unsafe { ResetEvent(comm_event) };

                let mut errors: u32 = 0;
                let mut comstat = MaybeUninit::<COMSTAT>::uninit();
                unsafe { ClearCommError(handle, &mut errors, comstat.as_mut_ptr()) };
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
                            GetOverlappedResult(handle, &mut read_overlapped, &mut bytes_read, TRUE)
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
                            &sender,
                        );
                    }
                }
            } else {
                // stop_event またはエラー
                unsafe { CancelIo(handle) };
                break;
            }
        }

        cleanup_thread();
        Ok(())
    }
}

// ========== 非Windows用フォールバック ==========
#[cfg(not(target_os = "windows"))]
fn fallback_receiver_loop(
    mut port: Box<dyn serialport::SerialPort>,
    idle_threshold: Duration,
    byte_duration: Duration,
    sender: Sender<DataEntry>,
    stop: Receiver<()>,
) {
    let mut buf = [0u8; 256];
    let mut last_byte_time: Option<Instant> = None;

    loop {
        if stop.try_recv().is_ok() {
            break;
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
