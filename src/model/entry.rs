use std::time::Instant;

/// 受信データの1エントリ
#[derive(Clone, Debug)]
pub enum DataEntry {
    /// 受信バイト (値, 受信時刻)
    Byte(u8, Instant),
    /// アイドル検出 (持続時間ms)
    Idle(f64),
    /// 通信エラー（フレーミング、オーバーラン、パリティ等）
    Error,
    /// 送信バイト (値, 送信時刻)
    Sent(u8, Instant),
}
