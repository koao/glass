use std::time::Duration;

use serde::{Deserialize, Serialize};

/// シリアルポート設定
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SerialConfig {
    pub port_name: String,
    pub baud_rate: u32,
    pub data_bits: u8,
    pub parity: ParitySetting,
    pub stop_bits: StopBitsSetting,
}

impl SerialConfig {
    /// 1バイトの送信時間を計算（スタートビット + データビット + パリティ + ストップビット）
    pub fn byte_duration(&self) -> Duration {
        let bits = 1 + self.data_bits as u32 + self.parity.bit_count() + self.stop_bits.bit_count();
        Duration::from_secs_f64(bits as f64 / self.baud_rate as f64)
    }
}

impl Default for SerialConfig {
    fn default() -> Self {
        Self {
            port_name: String::new(),
            baud_rate: 9600,
            data_bits: 8,
            parity: ParitySetting::None,
            stop_bits: StopBitsSetting::One,
        }
    }
}

/// パリティ設定
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ParitySetting {
    None,
    Odd,
    Even,
}

impl ParitySetting {
    pub const ALL: &[Self] = &[Self::None, Self::Odd, Self::Even];

    pub fn bit_count(&self) -> u32 {
        match self {
            Self::None => 0,
            _ => 1,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Odd => "Odd",
            Self::Even => "Even",
        }
    }

    pub fn to_serialport(&self) -> serialport::Parity {
        match self {
            Self::None => serialport::Parity::None,
            Self::Odd => serialport::Parity::Odd,
            Self::Even => serialport::Parity::Even,
        }
    }
}

/// ストップビット設定
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum StopBitsSetting {
    One,
    Two,
}

impl StopBitsSetting {
    pub const ALL: &[Self] = &[Self::One, Self::Two];

    pub fn bit_count(&self) -> u32 {
        match self {
            Self::One => 1,
            Self::Two => 2,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::One => "1",
            Self::Two => "2",
        }
    }

    pub fn to_serialport(&self) -> serialport::StopBits {
        match self {
            Self::One => serialport::StopBits::One,
            Self::Two => serialport::StopBits::Two,
        }
    }
}

/// 利用可能なボーレート一覧
pub const BAUD_RATES: &[u32] = &[9600, 19200, 38400, 57600, 115200];

/// データビット選択肢
pub const DATA_BITS: &[u8] = &[7, 8];
