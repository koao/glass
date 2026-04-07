//! プロトコルフレームのチェックサム / CRC 検証
//!
//! TOML の `[[protocol.frame_rules]]` 内に `checksum` テーブルとして指定し、
//! engine 側で確定したフレームに対し `verify` を呼んで結果を保持する。

use serde::Deserialize;

/// 対応アルゴリズム
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChecksumAlgorithm {
    /// CRC-16/ARC: poly=0x8005, init=0x0000, refin=true, refout=true, xorout=0x0000
    /// (lemon ach/bch が採用)
    Crc16Arc,
    /// CRC-16/MODBUS: poly=0x8005, init=0xFFFF, refin=true, refout=true, xorout=0x0000
    Crc16Modbus,
    /// CRC-16/CCITT-FALSE: poly=0x1021, init=0xFFFF, refin=false, refout=false, xorout=0x0000
    Crc16CcittFalse,
    /// CRC-16/XMODEM: poly=0x1021, init=0x0000, refin=false, refout=false, xorout=0x0000
    Crc16Xmodem,
    /// CRC-8 (SMBus): poly=0x07, init=0x00
    Crc8,
    /// 単純 8bit 加算（下位 8bit のみ採用）
    Sum8,
    /// 単純 XOR
    Xor8,
    /// BCC: 8bit 加算の二の補数
    Bcc,
}

impl ChecksumAlgorithm {
    pub fn label(self) -> &'static str {
        match self {
            ChecksumAlgorithm::Crc16Arc => "CRC-16/ARC",
            ChecksumAlgorithm::Crc16Modbus => "CRC-16/MODBUS",
            ChecksumAlgorithm::Crc16CcittFalse => "CRC-16/CCITT-FALSE",
            ChecksumAlgorithm::Crc16Xmodem => "CRC-16/XMODEM",
            ChecksumAlgorithm::Crc8 => "CRC-8",
            ChecksumAlgorithm::Sum8 => "SUM8",
            ChecksumAlgorithm::Xor8 => "XOR8",
            ChecksumAlgorithm::Bcc => "BCC",
        }
    }

    /// 標準サイズ（バイト数）
    fn default_size(self) -> usize {
        match self {
            ChecksumAlgorithm::Crc16Arc
            | ChecksumAlgorithm::Crc16Modbus
            | ChecksumAlgorithm::Crc16CcittFalse
            | ChecksumAlgorithm::Crc16Xmodem => 2,
            _ => 1,
        }
    }
}

/// 計算対象範囲を指定する DSL
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChecksumRange {
    /// trigger byte の直後 〜 末尾の checksum 直前
    /// （end_byte があれば end_byte は含まれる）
    AfterTriggerToEnd,
    /// trigger byte 〜 末尾の checksum 直前（trigger 含む）
    TriggerToEnd,
    /// trigger byte の直後 〜 end_byte の直前（end_byte 除外）
    AfterTriggerBeforeEnd,
    /// フレーム全体から末尾 checksum を除いた部分
    WholeFrameExcludingChecksum,
}

/// エンディアン（16bit 系のみ意味あり）
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChecksumEndian {
    Little,
    #[default]
    Big,
}

/// チェックサム仕様
#[derive(Clone, Debug, Deserialize)]
pub struct ChecksumSpec {
    pub algorithm: ChecksumAlgorithm,
    pub range: ChecksumRange,
    /// バイト数（省略時はアルゴリズム既定）
    #[serde(default)]
    pub size: Option<usize>,
    #[serde(default)]
    pub endian: ChecksumEndian,
}

impl ChecksumSpec {
    pub fn effective_size(&self) -> usize {
        self.size.unwrap_or_else(|| self.algorithm.default_size())
    }
}

/// 検証結果
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChecksumStatus {
    Valid {
        value: u64,
    },
    Invalid {
        expected: u64,
        actual: u64,
    },
    /// フレームが短すぎる等で算出不能
    NotApplicable,
}

/// 計算範囲を切り出す
fn slice_for<'a>(spec: &ChecksumSpec, frame: &'a [u8], end_byte: Option<u8>) -> Option<&'a [u8]> {
    let cs_size = spec.effective_size();
    if frame.len() <= cs_size {
        return None;
    }
    let body = &frame[..frame.len() - cs_size]; // 末尾 checksum を除いたフレーム本体

    match spec.range {
        ChecksumRange::WholeFrameExcludingChecksum => Some(body),
        ChecksumRange::TriggerToEnd => Some(body),
        ChecksumRange::AfterTriggerToEnd => {
            if body.is_empty() {
                None
            } else {
                Some(&body[1..])
            }
        }
        ChecksumRange::AfterTriggerBeforeEnd => {
            let end_byte = end_byte?;
            if body.is_empty() {
                return None;
            }
            // body 内で末尾の end_byte を探す
            let end_pos = body.iter().rposition(|b| *b == end_byte)?;
            if end_pos == 0 {
                return None;
            }
            Some(&body[1..end_pos])
        }
    }
}

/// フレーム末尾から期待値を取り出す
fn extract_expected(spec: &ChecksumSpec, frame: &[u8]) -> Option<u64> {
    let cs_size = spec.effective_size();
    if frame.len() < cs_size {
        return None;
    }
    let tail = &frame[frame.len() - cs_size..];
    let mut v: u64 = 0;
    match spec.endian {
        ChecksumEndian::Big => {
            for &b in tail {
                v = (v << 8) | b as u64;
            }
        }
        ChecksumEndian::Little => {
            for &b in tail.iter().rev() {
                v = (v << 8) | b as u64;
            }
        }
    }
    Some(v)
}

/// 計算
fn compute(spec: &ChecksumSpec, data: &[u8]) -> u64 {
    match spec.algorithm {
        ChecksumAlgorithm::Crc16Arc => crc16_reflected(data, 0x0000) as u64,
        ChecksumAlgorithm::Crc16Modbus => crc16_reflected(data, 0xFFFF) as u64,
        ChecksumAlgorithm::Crc16CcittFalse => crc16_ccitt(data, 0xFFFF) as u64,
        ChecksumAlgorithm::Crc16Xmodem => crc16_ccitt(data, 0x0000) as u64,
        ChecksumAlgorithm::Crc8 => crc8_smbus(data) as u64,
        ChecksumAlgorithm::Sum8 => {
            let mut s: u8 = 0;
            for &b in data {
                s = s.wrapping_add(b);
            }
            s as u64
        }
        ChecksumAlgorithm::Xor8 => {
            let mut s: u8 = 0;
            for &b in data {
                s ^= b;
            }
            s as u64
        }
        ChecksumAlgorithm::Bcc => {
            let mut s: u8 = 0;
            for &b in data {
                s = s.wrapping_add(b);
            }
            (s.wrapping_neg()) as u64
        }
    }
}

/// CRC-16 反射多項式版（poly=0xA001 = reflect(0x8005)）。
/// ARC, MODBUS が共有。`init` のみ切り替える。
fn crc16_reflected(data: &[u8], init: u16) -> u16 {
    let mut crc = init;
    for &b in data {
        crc ^= b as u16;
        for _ in 0..8 {
            if crc & 0x0001 != 0 {
                crc = (crc >> 1) ^ 0xA001;
            } else {
                crc >>= 1;
            }
        }
    }
    crc
}

/// CRC-16/CCITT 系（poly=0x1021、refin/refout=false）
fn crc16_ccitt(data: &[u8], init: u16) -> u16 {
    let mut crc = init;
    for &b in data {
        crc ^= (b as u16) << 8;
        for _ in 0..8 {
            if crc & 0x8000 != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

/// CRC-8/SMBus: poly=0x07, init=0x00
fn crc8_smbus(data: &[u8]) -> u8 {
    let mut crc: u8 = 0;
    for &b in data {
        crc ^= b;
        for _ in 0..8 {
            if crc & 0x80 != 0 {
                crc = (crc << 1) ^ 0x07;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

/// フレーム全体に対し検証を実行する
pub fn verify(spec: &ChecksumSpec, frame: &[u8], end_byte: Option<u8>) -> ChecksumStatus {
    let Some(data) = slice_for(spec, frame, end_byte) else {
        return ChecksumStatus::NotApplicable;
    };
    let Some(expected) = extract_expected(spec, frame) else {
        return ChecksumStatus::NotApplicable;
    };
    let actual = compute(spec, data);
    if actual == expected {
        ChecksumStatus::Valid { value: actual }
    } else {
        ChecksumStatus::Invalid { expected, actual }
    }
}

/// バイト数に応じた HEX 文字列フォーマット（2/4/etc 桁）
pub fn format_value(value: u64, size: usize) -> String {
    let width = size * 2;
    format!("{:0width$X}", value, width = width)
}

#[cfg(test)]
mod tests {
    use super::*;

    // 既知ベクトル: "123456789" (ASCII 9バイト)
    const CHECK: &[u8] = b"123456789";

    #[test]
    fn vec_crc16_arc() {
        // 既知値: 0xBB3D
        assert_eq!(crc16_reflected(CHECK, 0x0000), 0xBB3D);
    }

    #[test]
    fn vec_crc16_modbus() {
        // 既知値: 0x4B37
        assert_eq!(crc16_reflected(CHECK, 0xFFFF), 0x4B37);
    }

    #[test]
    fn vec_crc16_ccitt_false() {
        // 既知値: 0x29B1
        assert_eq!(crc16_ccitt(CHECK, 0xFFFF), 0x29B1);
    }

    #[test]
    fn vec_crc16_xmodem() {
        // 既知値: 0x31C3
        assert_eq!(crc16_ccitt(CHECK, 0x0000), 0x31C3);
    }

    #[test]
    fn vec_crc8_smbus() {
        // 既知値: 0xF4
        assert_eq!(crc8_smbus(CHECK), 0xF4);
    }

    #[test]
    fn vec_sum_xor_bcc() {
        // SUM8: 0x31..0x39 の和 = 0x01CD → 下位 0xCD
        let mut sum: u8 = 0;
        for &b in CHECK {
            sum = sum.wrapping_add(b);
        }
        assert_eq!(sum, 0xDD);
        // XOR
        let mut x: u8 = 0;
        for &b in CHECK {
            x ^= b;
        }
        assert_eq!(x, 0x31);
        // BCC
        assert_eq!(sum.wrapping_neg(), 0x23);
    }

    fn ach_spec_le() -> ChecksumSpec {
        ChecksumSpec {
            algorithm: ChecksumAlgorithm::Crc16Arc,
            range: ChecksumRange::AfterTriggerToEnd,
            size: Some(2),
            endian: ChecksumEndian::Little,
        }
    }

    #[test]
    fn verify_ach_like_frame_valid() {
        // STX(02) + 種別'0' + アドレス'01' + データ'AB' + ETX(03) + CRC(LE)
        let body = [b'0', b'0', b'1', b'A', b'B', 0x03];
        let crc = crc16_reflected(&body, 0x0000);
        let mut frame = vec![0x02];
        frame.extend_from_slice(&body);
        frame.push((crc & 0xFF) as u8);
        frame.push((crc >> 8) as u8);

        let status = verify(&ach_spec_le(), &frame, Some(0x03));
        assert!(
            matches!(status, ChecksumStatus::Valid { .. }),
            "{:?}",
            status
        );
    }

    #[test]
    fn verify_ach_like_frame_invalid() {
        let body = [b'0', b'0', b'1', b'A', b'B', 0x03];
        let crc = crc16_reflected(&body, 0x0000);
        let mut frame = vec![0x02];
        frame.extend_from_slice(&body);
        // 故意に壊す
        frame.push(((crc & 0xFF) as u8) ^ 0xFF);
        frame.push((crc >> 8) as u8);

        let status = verify(&ach_spec_le(), &frame, Some(0x03));
        assert!(
            matches!(status, ChecksumStatus::Invalid { .. }),
            "{:?}",
            status
        );
    }

    #[test]
    fn verify_too_short_frame() {
        let frame = [0x02, 0x03];
        let status = verify(&ach_spec_le(), &frame, Some(0x03));
        assert_eq!(status, ChecksumStatus::NotApplicable);
    }

    #[test]
    fn range_after_trigger_before_end_excludes_end_byte() {
        // BCC = 二の補数 over (data without trigger nor end_byte nor checksum)
        let spec = ChecksumSpec {
            algorithm: ChecksumAlgorithm::Bcc,
            range: ChecksumRange::AfterTriggerBeforeEnd,
            size: Some(1),
            endian: ChecksumEndian::Big,
        };
        // STX + 'A' 'B' + ETX + BCC
        let bcc = (b'A'.wrapping_add(b'B')).wrapping_neg();
        let frame = vec![0x02u8, b'A', b'B', 0x03, bcc];
        assert!(matches!(
            verify(&spec, &frame, Some(0x03)),
            ChecksumStatus::Valid { .. }
        ));
    }

    #[test]
    fn endian_big_vs_little() {
        // 期待値の取り出しエンディアンが効くこと
        let frame = [0x00u8, 0x12, 0x34];
        let spec_be = ChecksumSpec {
            algorithm: ChecksumAlgorithm::Sum8,
            range: ChecksumRange::WholeFrameExcludingChecksum,
            size: Some(2),
            endian: ChecksumEndian::Big,
        };
        assert_eq!(extract_expected(&spec_be, &frame), Some(0x1234));
        let mut spec_le = spec_be.clone();
        spec_le.endian = ChecksumEndian::Little;
        assert_eq!(extract_expected(&spec_le, &frame), Some(0x3412));
    }
}
