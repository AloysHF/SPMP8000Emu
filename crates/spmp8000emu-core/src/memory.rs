// Memory management for SPMP8000 emulator
//
// SPMP8000 memory map:
// - 0x00000000 - 0x00FFFFFF: 16MB RAM (code + data)
// - 0x009FF000 - 0x009FFFFF: HLE function table and trampolines
// - 0x00A00000:              Code load address (from libgame.ld)
// - 0x01000000 - 0x01FFFFFF: 16MB Video memory / framebuffer
// - 0x02000000 - 0x02FFFFFF: Peripheral registers
// - 0x00280000 - 0x00A00000: Firmware area (FW_START - FW_END)

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Memory permissions (simplified)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Permission {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}

impl Permission {
    pub const READ: Permission = Permission {
        read: true,
        write: false,
        execute: false,
    };
    pub const WRITE: Permission = Permission {
        read: false,
        write: true,
        execute: false,
    };
    pub const EXEC: Permission = Permission {
        read: false,
        write: false,
        execute: true,
    };
    pub const ALL: Permission = Permission {
        read: true,
        write: true,
        execute: true,
    };
}

impl std::ops::BitOr for Permission {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        Permission {
            read: self.read || rhs.read,
            write: self.write || rhs.write,
            execute: self.execute || rhs.execute,
        }
    }
}

/// Memory region with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRegion {
    pub base: u32,
    pub size: u32,
    pub data: Vec<u8>,
    pub permissions: Permission,
    pub name: String,
}

/// A memory range held at a fixed value by an active cheat freeze.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrozenRange {
    address: u32,
    value: u32,
    width_bytes: u8,
}

impl FrozenRange {
    /// Create a little-endian range covering one to four bytes.
    #[must_use]
    pub fn new(address: u32, value: u32, width_bytes: u8) -> Self {
        debug_assert!((1..=4).contains(&width_bytes));
        Self {
            address,
            value,
            width_bytes,
        }
    }

    pub fn address(self) -> u32 {
        self.address
    }

    pub fn value(self) -> u32 {
        self.value
    }

    pub fn width_bytes(self) -> u8 {
        self.width_bytes
    }

    fn end(self) -> u32 {
        self.address.saturating_add(u32::from(self.width_bytes))
    }

    fn contains(self, address: u32) -> bool {
        address >= self.address && address < self.end()
    }

    fn byte_at(self, address: u32) -> u8 {
        (self.value >> ((address - self.address) * 8)) as u8
    }
}

/// Main memory manager
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    regions: Vec<MemoryRegion>,
    /// Register file (R0-R15, CPSR)
    registers: [u32; 17],
    /// Frozen ranges are owned by the active frontend cheat rules and are not
    /// part of a save state.
    #[serde(skip)]
    frozen: Vec<FrozenRange>,
}

/// Register indices
pub const REG_R0: usize = 0;
pub const REG_R1: usize = 1;
pub const REG_R2: usize = 2;
pub const REG_R3: usize = 3;
pub const REG_R4: usize = 4;
pub const REG_R5: usize = 5;
pub const REG_R6: usize = 6;
pub const REG_R7: usize = 7;
pub const REG_R8: usize = 8;
pub const REG_R9: usize = 9;
pub const REG_R10: usize = 10;
pub const REG_R11: usize = 11;
pub const REG_R12: usize = 12;
pub const REG_SP: usize = 13;
pub const REG_LR: usize = 14;
pub const REG_PC: usize = 15;
pub const REG_CPSR: usize = 16;

/// Well-known memory addresses
pub const CODE_LOAD_ADDR: u32 = 0x00A00000;
pub const RAM_BASE: u32 = 0x00000000;
pub const RAM_SIZE: u32 = 16 * 1024 * 1024; // 16MB
pub const FUNC_TABLE_SIZE: u32 = 4096;
pub const FUNC_TABLE_BASE: u32 = CODE_LOAD_ADDR - FUNC_TABLE_SIZE;
pub const VRAM_BASE: u32 = 0x01000000;
pub const VRAM_SIZE: u32 = 16 * 1024 * 1024; // 16MB
pub const PERIPHERAL_BASE: u32 = 0x02000000;
pub const PERIPHERAL_SIZE: u32 = 16 * 1024 * 1024; // 16MB
pub const KEY_STATE_ADDR: u32 = 0x00200000; // Address for key state

impl Memory {
    /// Create a new memory manager with default regions
    pub fn new() -> Self {
        Self {
            regions: Vec::new(),
            registers: [0; 17],
            frozen: Vec::new(),
        }
    }

    /// Initialize default memory regions
    pub fn init_default(&mut self) -> Result<()> {
        // Main RAM
        self.map_region(RAM_BASE, RAM_SIZE, Permission::ALL, "RAM")?;

        // Video RAM
        self.map_region(
            VRAM_BASE,
            VRAM_SIZE,
            Permission::READ | Permission::WRITE,
            "VRAM",
        )?;

        // Memory-mapped peripheral registers.
        self.map_region(
            PERIPHERAL_BASE,
            PERIPHERAL_SIZE,
            Permission::READ | Permission::WRITE,
            "PERIPHERAL",
        )?;

        Ok(())
    }

    /// Map a new memory region
    pub fn map_region(
        &mut self,
        base: u32,
        size: u32,
        permissions: Permission,
        name: &str,
    ) -> Result<()> {
        // Check for overlapping regions
        for region in &self.regions {
            if base < region.base + region.size && base + size > region.base {
                anyhow::bail!(
                    "Memory region overlaps with existing region '{}' at 0x{:08X}",
                    region.name,
                    region.base
                );
            }
        }

        self.regions.push(MemoryRegion {
            base,
            size,
            data: vec![0; size as usize],
            permissions,
            name: name.to_string(),
        });

        log::info!(
            "Mapped memory region: {} at 0x{:08X} ({} bytes)",
            name,
            base,
            size
        );
        Ok(())
    }

    /// Find the region containing the given address
    fn find_region(&self, addr: u32) -> Option<&MemoryRegion> {
        self.regions
            .iter()
            .find(|r| addr >= r.base && addr < r.base + r.size)
    }

    /// Find the mutable region containing the given address
    fn find_region_mut(&mut self, addr: u32) -> Option<&mut MemoryRegion> {
        self.regions
            .iter_mut()
            .find(|r| addr >= r.base && addr < r.base + r.size)
    }

    fn region_data(&self, base: u32, size: u32) -> Option<&[u8]> {
        self.regions
            .iter()
            .find(|region| region.base == base && region.size == size)
            .map(|region| region.data.as_slice())
    }

    fn region_data_mut(&mut self, base: u32, size: u32) -> Option<&mut [u8]> {
        self.regions
            .iter_mut()
            .find(|region| region.base == base && region.size == size)
            .map(|region| region.data.as_mut_slice())
    }

    fn region_for_range(&self, addr: u32, size: usize) -> Option<&MemoryRegion> {
        let size = u32::try_from(size).ok()?;
        if size == 0 {
            return None;
        }
        let end = addr.checked_add(size)?;
        self.regions.iter().find(|region| {
            let region_end = region.base.checked_add(region.size);
            addr >= region.base && region_end.is_some_and(|region_end| end <= region_end)
        })
    }

    /// Read a single byte
    pub fn read_u8(&self, addr: u32) -> Result<u8> {
        let region = self
            .find_region(addr)
            .ok_or_else(|| anyhow::anyhow!("Read from unmapped address: 0x{:08X}", addr))?;
        let offset = (addr - region.base) as usize;
        Ok(region.data[offset])
    }

    /// Write a single byte
    pub fn write_u8(&mut self, addr: u32, value: u8) -> Result<()> {
        let region = self
            .find_region_mut(addr)
            .ok_or_else(|| anyhow::anyhow!("Write to unmapped address: 0x{:08X}", addr))?;
        let offset = (addr - region.base) as usize;
        region.data[offset] = value;
        self.restore_frozen_byte(addr);
        Ok(())
    }

    /// Re-apply a frozen byte so writes cannot change a protected address.
    fn restore_frozen_byte(&mut self, addr: u32) {
        for index in 0..self.frozen.len() {
            let frozen = self.frozen[index];
            if !frozen.contains(addr) {
                continue;
            }
            let value = frozen.byte_at(addr);
            if let Some(region) = self.find_region_mut(addr) {
                let offset = (addr - region.base) as usize;
                region.data[offset] = value;
            }
        }
    }

    /// Read a 16-bit value (little-endian)
    pub fn read_u16(&self, addr: u32) -> Result<u16> {
        let lo = self.read_u8(addr)? as u16;
        let hi = self.read_u8(addr + 1)? as u16;
        Ok(lo | (hi << 8))
    }

    /// Write a 16-bit value (little-endian)
    pub fn write_u16(&mut self, addr: u32, value: u16) -> Result<()> {
        self.write_u8(addr, (value & 0xFF) as u8)?;
        self.write_u8(addr + 1, ((value >> 8) & 0xFF) as u8)?;
        Ok(())
    }

    /// Read a 32-bit value (little-endian)
    pub fn read_u32(&self, addr: u32) -> Result<u32> {
        let b0 = self.read_u8(addr)? as u32;
        let b1 = self.read_u8(addr + 1)? as u32;
        let b2 = self.read_u8(addr + 2)? as u32;
        let b3 = self.read_u8(addr + 3)? as u32;
        Ok(b0 | (b1 << 8) | (b2 << 16) | (b3 << 24))
    }

    /// Write a 32-bit value (little-endian)
    pub fn write_u32(&mut self, addr: u32, value: u32) -> Result<()> {
        self.write_u8(addr, (value & 0xFF) as u8)?;
        self.write_u8(addr + 1, ((value >> 8) & 0xFF) as u8)?;
        self.write_u8(addr + 2, ((value >> 16) & 0xFF) as u8)?;
        self.write_u8(addr + 3, ((value >> 24) & 0xFF) as u8)?;
        Ok(())
    }

    /// Read a block of memory
    pub fn read_block(&self, addr: u32, size: usize) -> Result<Vec<u8>> {
        let mut result = Vec::with_capacity(size);
        for i in 0..size {
            result.push(self.read_u8(addr + i as u32)?);
        }
        Ok(result)
    }

    /// Write a block of memory
    pub fn write_block(&mut self, addr: u32, data: &[u8]) -> Result<()> {
        for (i, &byte) in data.iter().enumerate() {
            self.write_u8(addr + i as u32, byte)?;
        }
        Ok(())
    }

    /// Read a null-terminated string from memory
    pub fn read_string(&self, addr: u32, max_len: usize) -> Result<String> {
        let mut bytes = Vec::new();
        for i in 0..max_len {
            let byte = self.read_u8(addr + i as u32)?;
            if byte == 0 {
                break;
            }
            bytes.push(byte);
        }
        Ok(String::from_utf8_lossy(&bytes).to_string())
    }

    /// Get register value
    pub fn get_register(&self, reg: usize) -> u32 {
        if reg < 17 {
            self.registers[reg]
        } else {
            0
        }
    }

    /// Set register value
    pub fn set_register(&mut self, reg: usize, value: u32) {
        if reg < 17 {
            self.registers[reg] = value;
        }
    }

    /// Get the contiguous 16 MiB system RAM exposed to frontends.
    pub fn system_ram(&self) -> Option<&[u8]> {
        self.region_data(RAM_BASE, RAM_SIZE)
    }

    /// Get mutable system RAM without changing its allocation.
    pub fn system_ram_mut(&mut self) -> Option<&mut [u8]> {
        self.region_data_mut(RAM_BASE, RAM_SIZE)
    }

    /// Get the contiguous 16 MiB video RAM exposed to frontends.
    pub fn video_ram(&self) -> Option<&[u8]> {
        self.region_data(VRAM_BASE, VRAM_SIZE)
    }

    /// Get mutable video RAM without changing its allocation.
    pub fn video_ram_mut(&mut self) -> Option<&mut [u8]> {
        self.region_data_mut(VRAM_BASE, VRAM_SIZE)
    }

    /// Get the memory-mapped peripheral storage used by memory descriptors.
    pub fn peripheral_memory(&self) -> Option<&[u8]> {
        self.region_data(PERIPHERAL_BASE, PERIPHERAL_SIZE)
    }

    /// Get mutable memory-mapped peripheral storage.
    pub fn peripheral_memory_mut(&mut self) -> Option<&mut [u8]> {
        self.region_data_mut(PERIPHERAL_BASE, PERIPHERAL_SIZE)
    }

    /// Check whether a cheat can safely write a complete RAM or VRAM value.
    pub fn is_cheat_writable_range(&self, addr: u32, size: usize) -> bool {
        self.region_for_range(addr, size).is_some_and(|region| {
            region.permissions.write && matches!(region.base, RAM_BASE | VRAM_BASE)
        })
    }

    /// Replace the write-protected ranges held by active memory freezes.
    ///
    /// Every later write that overlaps one of these ranges is reverted in
    /// place, so the game cannot change the value between frames.
    pub fn set_frozen_ranges(&mut self, ranges: Vec<FrozenRange>) {
        self.frozen = ranges;
    }

    /// Copy a validated memory state while preserving frontend-visible pointers.
    pub(crate) fn copy_state_from(&mut self, source: &Self) -> Result<()> {
        self.validate_state()?;
        source.validate_state()?;
        for (destination, source) in self.regions.iter_mut().zip(&source.regions) {
            destination.data.copy_from_slice(&source.data);
        }
        self.registers = source.registers;
        Ok(())
    }

    /// Get all regions (for debugging)
    pub fn regions(&self) -> &[MemoryRegion] {
        &self.regions
    }

    pub(crate) fn validate_state(&self) -> Result<()> {
        let expected = [
            (RAM_BASE, RAM_SIZE, Permission::ALL, "RAM"),
            (
                VRAM_BASE,
                VRAM_SIZE,
                Permission::READ | Permission::WRITE,
                "VRAM",
            ),
            (
                PERIPHERAL_BASE,
                PERIPHERAL_SIZE,
                Permission::READ | Permission::WRITE,
                "PERIPHERAL",
            ),
        ];
        if self.regions.len() != expected.len() {
            anyhow::bail!(
                "save state has {} memory regions, expected {}",
                self.regions.len(),
                expected.len()
            );
        }

        for (region, (base, size, permissions, name)) in self.regions.iter().zip(expected) {
            if region.base != base
                || region.size != size
                || region.permissions != permissions
                || region.name != name
                || region.data.len() != size as usize
            {
                anyhow::bail!("save state has an incompatible {name} memory region");
            }
        }
        Ok(())
    }
}

impl Default for Memory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_read_write() {
        let mut memory = Memory::new();
        memory
            .map_region(0x1000, 4096, Permission::ALL, "test")
            .unwrap();

        memory.write_u32(0x1000, 0x12345678).unwrap();
        assert_eq!(memory.read_u32(0x1000).unwrap(), 0x12345678);

        memory.write_u16(0x1004, 0xABCD).unwrap();
        assert_eq!(memory.read_u16(0x1004).unwrap(), 0xABCD);

        memory.write_u8(0x1006, 0x42).unwrap();
        assert_eq!(memory.read_u8(0x1006).unwrap(), 0x42);
    }

    #[test]
    fn test_memory_block_operations() {
        let mut memory = Memory::new();
        memory
            .map_region(0x0, 4096, Permission::ALL, "test")
            .unwrap();

        let data = vec![1, 2, 3, 4, 5];
        memory.write_block(0x100, &data).unwrap();

        let read_data = memory.read_block(0x100, 5).unwrap();
        assert_eq!(read_data, data);
    }

    #[test]
    fn test_memory_string() {
        let mut memory = Memory::new();
        memory
            .map_region(0x0, 4096, Permission::ALL, "test")
            .unwrap();

        let s = "Hello";
        for (i, c) in s.bytes().enumerate() {
            memory.write_u8(0x200 + i as u32, c).unwrap();
        }
        memory.write_u8(0x205, 0).unwrap();

        assert_eq!(memory.read_string(0x200, 32).unwrap(), "Hello");
    }

    #[test]
    fn test_memory_out_of_bounds() {
        let mut memory = Memory::new();
        memory
            .map_region(0x1000, 256, Permission::ALL, "test")
            .unwrap();

        assert!(memory.read_u32(0x2000).is_err());
        assert!(memory.write_u32(0x2000, 0).is_err());
    }

    #[test]
    fn default_regions_have_stable_frontend_slices() {
        let mut memory = Memory::new();
        memory.init_default().unwrap();

        assert_eq!(memory.system_ram().unwrap().len(), RAM_SIZE as usize);
        assert_eq!(memory.video_ram().unwrap().len(), VRAM_SIZE as usize);
        assert_eq!(
            memory.peripheral_memory().unwrap().len(),
            PERIPHERAL_SIZE as usize
        );
        memory.system_ram_mut().unwrap()[0x1234] = 0x5A;
        memory.video_ram_mut().unwrap()[0x20] = 0xA5;
        assert_eq!(memory.read_u8(RAM_BASE + 0x1234).unwrap(), 0x5A);
        assert_eq!(memory.read_u8(VRAM_BASE + 0x20).unwrap(), 0xA5);
    }

    #[test]
    fn state_copy_preserves_region_allocations() {
        let mut destination = Memory::new();
        destination.init_default().unwrap();
        let ram_pointer = destination.system_ram_mut().unwrap().as_mut_ptr();
        let vram_pointer = destination.video_ram_mut().unwrap().as_mut_ptr();

        let mut source = Memory::new();
        source.init_default().unwrap();
        source.write_u32(RAM_BASE + 4, 0x1234_5678).unwrap();
        source.write_u32(VRAM_BASE + 8, 0xCAFE_BABE).unwrap();
        source.set_register(REG_R5, 99);

        destination.copy_state_from(&source).unwrap();

        assert_eq!(
            destination.system_ram_mut().unwrap().as_mut_ptr(),
            ram_pointer
        );
        assert_eq!(
            destination.video_ram_mut().unwrap().as_mut_ptr(),
            vram_pointer
        );
        assert_eq!(destination.read_u32(RAM_BASE + 4).unwrap(), 0x1234_5678);
        assert_eq!(destination.read_u32(VRAM_BASE + 8).unwrap(), 0xCAFE_BABE);
        assert_eq!(destination.get_register(REG_R5), 99);
    }

    #[test]
    fn frozen_ranges_revert_overlapping_writes() {
        let mut memory = Memory::new();
        memory.init_default().unwrap();
        memory.set_frozen_ranges(vec![FrozenRange::new(RAM_BASE + 0x1004, 0x1234_5678, 4)]);

        memory.write_u8(RAM_BASE + 0x1004, 0xFF).unwrap();
        memory.write_u16(RAM_BASE + 0x1004, 0xFFFF).unwrap();
        memory.write_u32(RAM_BASE + 0x1004, 0).unwrap();
        assert_eq!(memory.read_u32(RAM_BASE + 0x1004).unwrap(), 0x1234_5678);

        memory.write_u8(RAM_BASE + 0x1008, 0xAA).unwrap();
        assert_eq!(memory.read_u8(RAM_BASE + 0x1008).unwrap(), 0xAA);
    }

    #[test]
    fn frozen_ranges_block_partial_overlap_and_can_be_cleared() {
        let mut memory = Memory::new();
        memory.init_default().unwrap();
        memory.set_frozen_ranges(vec![FrozenRange::new(RAM_BASE + 0x1004, 0x1234, 2)]);

        memory.write_u32(RAM_BASE + 0x1002, 0xFFFF_FFFF).unwrap();
        assert_eq!(memory.read_u16(RAM_BASE + 0x1002).unwrap(), 0xFFFF);
        assert_eq!(memory.read_u16(RAM_BASE + 0x1004).unwrap(), 0x1234);

        memory.set_frozen_ranges(Vec::new());
        memory.write_u16(RAM_BASE + 0x1004, 7).unwrap();
        assert_eq!(memory.read_u16(RAM_BASE + 0x1004).unwrap(), 7);
    }
}
