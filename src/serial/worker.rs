use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};

use crate::model::entry::DataEntry;
use crate::serial::config::SerialConfig;

/// 受信スレッドを起動する
///
/// 戻り値: スレッドハンドル
/// エラー時: ポートオープン失敗
pub fn spawn_receiver(
    config: &SerialConfig,
    idle_threshold: Duration,
    sender: Sender<DataEntry>,
    stop: Receiver<()>,
) -> Result<std::thread::JoinHandle<()>, serialport::Error> {
    let port = serialport::new(&config.port_name, config.baud_rate)
        .data_bits(match config.data_bits {
            7 => serialport::DataBits::Seven,
            _ => serialport::DataBits::Eight,
        })
        .parity(config.parity.to_serialport())
        .stop_bits(config.stop_bits.to_serialport())
        .timeout(Duration::from_millis(10))
        .open()?;

    let handle = std::thread::spawn(move || {
        receiver_loop(port, idle_threshold, sender, stop);
    });

    Ok(handle)
}

/// 受信ループ本体
fn receiver_loop(
    mut port: Box<dyn serialport::SerialPort>,
    idle_threshold: Duration,
    sender: Sender<DataEntry>,
    stop: Receiver<()>,
) {
    let mut buf = [0u8; 1024];
    let mut last_byte_time: Option<Instant> = None;

    loop {
        // 停止信号チェック
        if stop.try_recv().is_ok() {
            break;
        }

        match port.read(&mut buf) {
            Ok(n) if n > 0 => {
                let now = Instant::now();
                for &byte in &buf[..n] {
                    // IDLE検出: 前回のバイトからの経過時間が閾値を超えた場合
                    if let Some(last) = last_byte_time {
                        let elapsed = now.duration_since(last);
                        if elapsed >= idle_threshold {
                            let ms = elapsed.as_secs_f64() * 1000.0;
                            if sender.send(DataEntry::Idle(ms)).is_err() {
                                return;
                            }
                        }
                    }
                    if sender.send(DataEntry::Byte(byte, now)).is_err() {
                        return;
                    }
                    last_byte_time = Some(now);
                }
            }
            Ok(_) => {}
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {
                // タイムアウト: データなし、ループ継続
            }
            Err(_) => {
                // その他のエラー: スレッド終了
                break;
            }
        }
    }
}
