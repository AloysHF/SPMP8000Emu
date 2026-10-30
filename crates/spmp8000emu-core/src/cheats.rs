// Shared memory and register cheat support for all frontends.

use std::collections::BTreeMap;
use std::str::FromStr;

use thiserror::Error;

use crate::arm_cpu::ArmCpu;
use crate::memory::{FrozenRange, Memory, REG_CPSR};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CheatParseError {
    #[error("cheat code is empty")]
    Empty,
    #[error("cheat code must use '<target>=<value>' syntax")]
    MissingValue,
    #[error("unknown cheat target '{0}'")]
    UnknownTarget(String),
    #[error("invalid numeric value '{0}'")]
    InvalidNumber(String),
    #[error("{0}-bit cheat value is out of range")]
    ValueOutOfRange(u32),
    #[error("{0}-bit memory address 0x{1:08X} is not aligned")]
    MisalignedAddress(u32, u32),
    #[error("memory range 0x{address:08X}..0x{end:08X} is not writable RAM or VRAM")]
    InvalidMemoryRange { address: u32, end: u32 },
    #[error("unknown ARM register '{0}'")]
    InvalidRegister(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryWidth {
    U8,
    U16,
    U32,
}

impl MemoryWidth {
    /// Byte width of the access.
    pub const fn bytes(self) -> u32 {
        match self {
            Self::U8 => 1,
            Self::U16 => 2,
            Self::U32 => 4,
        }
    }

    /// Bit width of the access, as used in the `<target>=<value>` syntax.
    pub const fn bits(self) -> u32 {
        self.bytes() * 8
    }

    const fn max_value(self) -> u64 {
        match self {
            Self::U8 => u8::MAX as u64,
            Self::U16 => u16::MAX as u64,
            Self::U32 => u32::MAX as u64,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArmRegister {
    General(u32),
    Cpsr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheatRule {
    Memory {
        width: MemoryWidth,
        address: u32,
        value: u32,
    },
    Register {
        register: ArmRegister,
        value: u32,
    },
}

impl CheatRule {
    fn validate(&self, memory: &Memory) -> Result<(), CheatParseError> {
        let Self::Memory { width, address, .. } = self else {
            return Ok(());
        };

        let bytes = width.bytes();
        if address % bytes != 0 {
            return Err(CheatParseError::MisalignedAddress(width.bits(), *address));
        }

        if !memory.is_cheat_writable_range(*address, bytes as usize) {
            let end = address.saturating_add(bytes - 1);
            return Err(CheatParseError::InvalidMemoryRange {
                address: *address,
                end,
            });
        }
        Ok(())
    }

    fn apply(&self, memory: &mut Memory, cpu: &mut ArmCpu) {
        match *self {
            Self::Memory {
                width,
                address,
                value,
            } => {
                let result = match width {
                    MemoryWidth::U8 => memory.write_u8(address, value as u8),
                    MemoryWidth::U16 => memory.write_u16(address, value as u16),
                    MemoryWidth::U32 => memory.write_u32(address, value),
                };
                if let Err(error) = result {
                    log::warn!("Failed to apply memory cheat at 0x{address:08X}: {error}");
                }
            }
            Self::Register { register, value } => match register {
                ArmRegister::General(index) => {
                    cpu.regs.set(index, value);
                    memory.set_register(index as usize, value);
                }
                ArmRegister::Cpsr => {
                    cpu.regs.cpsr = value;
                    memory.set_register(REG_CPSR, value);
                }
            },
        }
    }
}

impl FromStr for CheatRule {
    type Err = CheatParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let input = input.trim();
        if input.is_empty() {
            return Err(CheatParseError::Empty);
        }

        let (target, value) = input.split_once('=').ok_or(CheatParseError::MissingValue)?;
        let target = target.trim();
        let value = parse_number(value)?;
        let target_lower = target.to_ascii_lowercase();

        for (prefix, width) in [
            ("mem8:", MemoryWidth::U8),
            ("mem16:", MemoryWidth::U16),
            ("mem32:", MemoryWidth::U32),
        ] {
            if let Some(address) = target_lower.strip_prefix(prefix) {
                if value > width.max_value() {
                    return Err(CheatParseError::ValueOutOfRange(width.bits()));
                }
                return Ok(Self::Memory {
                    width,
                    address: parse_u32(address)?,
                    value: value as u32,
                });
            }
        }

        if let Some(register) = target_lower.strip_prefix("reg:") {
            return Ok(Self::Register {
                register: parse_register(register)?,
                value: u32::try_from(value).map_err(|_| CheatParseError::ValueOutOfRange(32))?,
            });
        }

        Err(CheatParseError::UnknownTarget(target.to_string()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheatSlot {
    pub enabled: bool,
    pub code: String,
    pub rule: CheatRule,
}

/// A frozen memory rule whose value the game changed before the next freeze.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheatClobber {
    pub index: u32,
    pub width: MemoryWidth,
    pub address: u32,
    pub expected: u32,
    pub actual: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CheatManager {
    slots: BTreeMap<u32, CheatSlot>,
}

impl CheatManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        log::debug!("Cleared {} cheat slots", self.slots.len());
        self.slots.clear();
    }

    pub fn set_slot(
        &mut self,
        index: u32,
        enabled: bool,
        code: &str,
        memory: &Memory,
    ) -> Result<(), CheatParseError> {
        let code = code.trim();
        if code.is_empty() {
            self.slots.remove(&index);
            return Ok(());
        }

        let rule = CheatRule::from_str(code)?;
        rule.validate(memory)?;
        log::debug!("Cheat slot {index} set: enabled={enabled} code='{code}'");
        self.slots.insert(
            index,
            CheatSlot {
                enabled,
                code: code.to_string(),
                rule,
            },
        );
        Ok(())
    }

    pub fn add_code(&mut self, code: &str, memory: &Memory) -> Result<u32, CheatParseError> {
        let index = self
            .slots
            .keys()
            .next_back()
            .map_or(0, |highest| highest.saturating_add(1));
        self.set_slot(index, true, code, memory)?;
        Ok(index)
    }

    pub fn get_slot(&self, index: u32) -> Option<&CheatSlot> {
        self.slots.get(&index)
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Report enabled memory rules whose value differs from the frozen one.
    ///
    /// Enabled memory rules write-protect their address, so this normally
    /// stays empty. A non-empty report means a writer bypassed the normal CPU
    /// write path, such as a frontend writing through a raw memory pointer.
    /// Call this before [`Self::apply`] to inspect the state left by such a
    /// writer.
    pub fn detect_clobbered(&self, memory: &Memory) -> Vec<CheatClobber> {
        let mut clobbered = Vec::new();
        for (&index, slot) in &self.slots {
            if !slot.enabled {
                continue;
            }
            let CheatRule::Memory {
                width,
                address,
                value,
            } = slot.rule
            else {
                continue;
            };
            let actual = match width {
                MemoryWidth::U8 => memory.read_u8(address).map(u32::from),
                MemoryWidth::U16 => memory.read_u16(address).map(u32::from),
                MemoryWidth::U32 => memory.read_u32(address),
            };
            if let Ok(actual) = actual {
                if actual != value {
                    clobbered.push(CheatClobber {
                        index,
                        width,
                        address,
                        expected: value,
                        actual,
                    });
                }
            }
        }
        clobbered
    }

    pub fn apply(&self, memory: &mut Memory, cpu: &mut ArmCpu) {
        for slot in self.slots.values().filter(|slot| slot.enabled) {
            slot.rule.apply(memory, cpu);
        }
    }

    /// Write-protected ranges for every enabled memory rule.
    pub fn frozen_ranges(&self) -> Vec<FrozenRange> {
        self.slots
            .values()
            .filter(|slot| slot.enabled)
            .filter_map(|slot| match slot.rule {
                CheatRule::Memory {
                    width,
                    address,
                    value,
                } => Some(FrozenRange::new(address, value, width.bytes() as u8)),
                CheatRule::Register { .. } => None,
            })
            .collect()
    }
}

fn parse_number(input: &str) -> Result<u64, CheatParseError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(CheatParseError::InvalidNumber(input.to_string()));
    }
    let result = input
        .strip_prefix("0x")
        .or_else(|| input.strip_prefix("0X"))
        .map_or_else(|| input.parse::<u64>(), |hex| u64::from_str_radix(hex, 16));
    result.map_err(|_| CheatParseError::InvalidNumber(input.to_string()))
}

fn parse_u32(input: &str) -> Result<u32, CheatParseError> {
    let value = parse_number(input)?;
    u32::try_from(value).map_err(|_| CheatParseError::InvalidNumber(input.trim().to_string()))
}

fn parse_register(input: &str) -> Result<ArmRegister, CheatParseError> {
    match input.trim().to_ascii_lowercase().as_str() {
        "sp" => Ok(ArmRegister::General(13)),
        "lr" => Ok(ArmRegister::General(14)),
        "pc" => Ok(ArmRegister::General(15)),
        "cpsr" => Ok(ArmRegister::Cpsr),
        register if register.starts_with('r') => register[1..]
            .parse::<u32>()
            .ok()
            .filter(|index| *index <= 15)
            .map(ArmRegister::General)
            .ok_or_else(|| CheatParseError::InvalidRegister(input.trim().to_string())),
        _ => Err(CheatParseError::InvalidRegister(input.trim().to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{PERIPHERAL_BASE, RAM_BASE, VRAM_BASE};

    fn memory() -> Memory {
        let mut memory = Memory::new();
        memory.init_default().unwrap();
        memory
    }

    #[test]
    fn parses_memory_and_register_rules() {
        assert_eq!(
            CheatRule::from_str("mem16:0x00123456=999").unwrap(),
            CheatRule::Memory {
                width: MemoryWidth::U16,
                address: 0x0012_3456,
                value: 999,
            }
        );
        assert_eq!(
            CheatRule::from_str("REG:sp=0x1234").unwrap(),
            CheatRule::Register {
                register: ArmRegister::General(13),
                value: 0x1234,
            }
        );
        assert_eq!(
            CheatRule::from_str("reg:cpsr=31").unwrap(),
            CheatRule::Register {
                register: ArmRegister::Cpsr,
                value: 31,
            }
        );
    }

    #[test]
    fn validates_width_alignment_and_mapped_ranges() {
        let memory = memory();
        let mut cheats = CheatManager::new();

        assert!(matches!(
            cheats.set_slot(0, true, "mem8:0x100=256", &memory),
            Err(CheatParseError::ValueOutOfRange(8))
        ));
        assert!(matches!(
            cheats.set_slot(0, true, "mem16:0x101=1", &memory),
            Err(CheatParseError::MisalignedAddress(16, 0x101))
        ));
        assert!(matches!(
            cheats.set_slot(
                0,
                true,
                &format!("mem32:0x{PERIPHERAL_BASE:08X}=1"),
                &memory
            ),
            Err(CheatParseError::InvalidMemoryRange { .. })
        ));
        assert!(matches!(
            cheats.set_slot(0, true, "mem32:0xFFFFFFFF=1", &memory),
            Err(CheatParseError::MisalignedAddress(32, 0xFFFF_FFFF))
        ));
    }

    #[test]
    fn applies_enabled_slots_in_index_order() {
        let mut memory = memory();
        let mut cpu = ArmCpu::new().unwrap();
        let mut cheats = CheatManager::new();
        cheats
            .set_slot(0, true, "mem32:0x00001000=0x12345678", &memory)
            .unwrap();
        cheats
            .set_slot(1, false, "mem32:0x00001000=0", &memory)
            .unwrap();
        cheats
            .set_slot(
                2,
                true,
                &format!("mem16:0x{:08X}=65535", VRAM_BASE + 2),
                &memory,
            )
            .unwrap();
        cheats
            .set_slot(3, true, "reg:r5=0xCAFEBABE", &memory)
            .unwrap();

        cheats.apply(&mut memory, &mut cpu);

        assert_eq!(memory.read_u32(RAM_BASE + 0x1000).unwrap(), 0x1234_5678);
        assert_eq!(memory.read_u16(VRAM_BASE + 2).unwrap(), u16::MAX);
        assert_eq!(cpu.regs.r5, 0xCAFE_BABE);
        assert_eq!(memory.get_register(5), 0xCAFE_BABE);
    }

    #[test]
    fn detects_enabled_memory_rules_overwritten_before_the_next_freeze() {
        let mut memory = memory();
        let mut cheats = CheatManager::new();
        let wide = RAM_BASE + 0x200;
        cheats.set_slot(0, true, "mem8:0x100=1", &memory).unwrap();
        cheats.set_slot(1, false, "mem8:0x101=2", &memory).unwrap();
        cheats
            .set_slot(2, true, &format!("mem32:0x{wide:08X}=0x11223344"), &memory)
            .unwrap();
        cheats.set_slot(3, true, "reg:r0=7", &memory).unwrap();

        memory.write_u8(0x100, 1).unwrap();
        memory.write_u32(wide, 0x1122_3344).unwrap();
        assert!(cheats.detect_clobbered(&memory).is_empty());

        memory.write_u8(0x100, 9).unwrap();
        memory.write_u8(0x101, 9).unwrap();
        memory.write_u32(wide, 0x5566_7788).unwrap();

        assert_eq!(
            cheats.detect_clobbered(&memory),
            vec![
                CheatClobber {
                    index: 0,
                    width: MemoryWidth::U8,
                    address: 0x100,
                    expected: 1,
                    actual: 9,
                },
                CheatClobber {
                    index: 2,
                    width: MemoryWidth::U32,
                    address: wide,
                    expected: 0x1122_3344,
                    actual: 0x5566_7788,
                },
            ]
        );

        let mut cpu = ArmCpu::new().unwrap();
        cheats.apply(&mut memory, &mut cpu);
        assert!(cheats.detect_clobbered(&memory).is_empty());
    }

    #[test]
    fn slots_can_be_replaced_disabled_removed_and_cleared() {
        let memory = memory();
        let mut cheats = CheatManager::new();
        cheats.set_slot(7, true, "mem8:0x100=1", &memory).unwrap();
        cheats.set_slot(7, false, "mem8:0x100=2", &memory).unwrap();
        assert_eq!(cheats.len(), 1);
        assert!(!cheats.get_slot(7).unwrap().enabled);
        assert_eq!(cheats.get_slot(7).unwrap().code, "mem8:0x100=2");

        cheats.set_slot(7, false, "", &memory).unwrap();
        assert!(cheats.is_empty());
        cheats.add_code("reg:r0=1", &memory).unwrap();
        cheats.clear();
        assert!(cheats.is_empty());
    }

    #[test]
    fn exposes_frozen_ranges_for_enabled_memory_rules() {
        let memory = memory();
        let mut cheats = CheatManager::new();
        cheats
            .set_slot(0, true, "mem16:0x00001000=30", &memory)
            .unwrap();
        cheats
            .set_slot(1, false, "mem8:0x00002000=1", &memory)
            .unwrap();
        cheats.set_slot(2, true, "reg:r0=1", &memory).unwrap();
        cheats
            .set_slot(3, true, "mem32:0x00003000=0xCAFEBABE", &memory)
            .unwrap();

        let ranges = cheats.frozen_ranges();
        assert_eq!(ranges.len(), 2);
        assert_eq!(
            (
                ranges[0].address(),
                ranges[0].value(),
                ranges[0].width_bytes()
            ),
            (0x1000, 30, 2)
        );
        assert_eq!(
            (
                ranges[1].address(),
                ranges[1].value(),
                ranges[1].width_bytes()
            ),
            (0x3000, 0xCAFE_BABE, 4)
        );
    }
}
