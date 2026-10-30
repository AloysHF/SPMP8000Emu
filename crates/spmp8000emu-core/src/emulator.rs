// Core emulator implementation
//
// This is the main emulator struct that ties all components together.
// It is platform-independent and shared by both the standalone frontend
// and the libretro core.

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::Arc;

use crate::api::NGameApi;
use crate::arm_cpu::ArmCpu;
use crate::audio_engine::AudioEngine;
use crate::bin_loader::{self, NGameHeader};
use crate::cheats::{CheatManager, CheatParseError};
use crate::config::{CoreConfig, UnknownInstructionPolicy};
use crate::decompressor;
use crate::function_table::FunctionTable;
use crate::input_handler::InputHandler;
use crate::memory::{self, Memory};
use crate::renderer::Renderer;

fn cheat_trace_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("SPMP8000_CHEAT_TRACE").is_some())
}

fn report_clobbered_cheats(cheats: &CheatManager, memory: &Memory) {
    for clobber in cheats.detect_clobbered(memory) {
        log::info!(
            "Cheat slot {}: mem{} at 0x{:08X} is {} (frozen at {}); the game overwrote it during the frame",
            clobber.index,
            clobber.width.bits(),
            clobber.address,
            clobber.actual,
            clobber.expected
        );
    }
}

/// Platform-independent emulator core
#[derive(Debug)]
pub struct Emulator {
    // Game info
    pub header: NGameHeader,
    pub game_path: PathBuf,
    boot_code: Arc<[u8]>,

    // Core components
    pub cpu: ArmCpu,
    pub memory: Memory,
    pub function_table: FunctionTable,
    pub api: NGameApi,

    // Output
    pub renderer: Renderer,
    pub audio: AudioEngine,
    pub input: InputHandler,
    pub cheats: CheatManager,
    pub config: CoreConfig,

    // State
    pub tick_count: u64,
    pub is_running: bool,
    pub exit_requested: bool,
}

impl Emulator {
    /// Create a new emulator from a game file path
    pub fn from_path(path: PathBuf, volume: u32) -> Result<Self> {
        Self::from_path_with_config(
            path,
            CoreConfig {
                volume,
                ..CoreConfig::default()
            },
        )
    }

    /// Create a new emulator using shared frontend configuration.
    pub fn from_path_with_config(path: PathBuf, config: CoreConfig) -> Result<Self> {
        log::info!("Loading game: {}", path.display());

        // Read the BIN file
        let data = std::fs::read(&path)
            .with_context(|| format!("Failed to read game file: {}", path.display()))?;

        // Parse header
        let header = bin_loader::parse_header(&data).context("Failed to parse NGame header")?;

        log::info!(
            "Game: {} ({} {}, {})",
            header.game_name,
            header.chip_type,
            header.version,
            header.media_type
        );

        // Extract and decompress data
        let compressed_data = bin_loader::extract_compressed_data(&data)?;
        let decompressed_data =
            decompressor::decompress(compressed_data).context("Failed to decompress game data")?;

        log::info!(
            "Decompressed: {} bytes (from {} bytes)",
            decompressed_data.len(),
            compressed_data.len()
        );

        let emu = Self::from_loaded_game(
            path,
            header,
            Arc::from(decompressed_data),
            config,
            InputHandler::new(),
        )?;

        log::info!("Emulator initialized successfully");
        Ok(emu)
    }

    fn from_loaded_game(
        path: PathBuf,
        header: NGameHeader,
        boot_code: Arc<[u8]>,
        config: CoreConfig,
        mut input: InputHandler,
    ) -> Result<Self> {
        let config = config.normalized();
        let mut cpu = ArmCpu::new().context("Failed to create ARM CPU")?;
        cpu.debug = config.debug_logging;
        let mut memory = Memory::new();
        memory
            .init_default()
            .context("Failed to initialize memory")?;

        let function_table = FunctionTable::new();
        Self::load_code(&mut memory, &boot_code)?;
        function_table.setup_in_memory(&mut memory)?;

        cpu.set_pc(memory::CODE_LOAD_ADDR)?;
        cpu.set_sp(0x00F00000)?;
        cpu.set_register(0, memory::FUNC_TABLE_BASE)?;

        let mut api = NGameApi::new();
        api.set_cpu_frequency(header.cpu_freq());
        if let Some(parent) = path.parent() {
            api.set_game_dir(&parent.to_string_lossy());
        }

        let resolution = header.default_resolution();
        let renderer = Renderer::new(resolution.0, resolution.1);
        let mut audio = AudioEngine::new(22050);
        audio.set_volume(config.volume);
        input.clear();

        Ok(Self {
            header,
            game_path: path,
            boot_code,
            cpu,
            memory,
            function_table,
            api,
            renderer,
            audio,
            input,
            cheats: CheatManager::new(),
            config,
            tick_count: 0,
            is_running: false,
            exit_requested: false,
        })
    }

    /// Load game code into memory
    fn load_code(memory: &mut Memory, code: &[u8]) -> Result<()> {
        // Load code at the standard load address
        let load_addr = memory::CODE_LOAD_ADDR;

        // Write code to memory
        memory
            .write_block(load_addr, code)
            .context("Failed to write game code")?;

        log::info!("Loaded {} bytes of code at 0x{:08X}", code.len(), load_addr);

        Ok(())
    }

    /// Get the game resolution
    pub fn get_resolution(&self) -> (u32, u32) {
        (self.renderer.width, self.renderer.height)
    }

    /// Get the audio sample rate
    pub fn get_audio_sample_rate(&self) -> f64 {
        self.audio.sample_rate as f64
    }

    /// Set button state from external input
    pub fn set_buttons(&mut self, buttons: u32) {
        let buttons = self.config.map_buttons(buttons);
        self.input.set_buttons(buttons);
        self.api.translate_buttons(self.input.get_buttons());
    }

    /// Apply frontend configuration without rebuilding runtime state.
    pub fn set_config(&mut self, config: CoreConfig) {
        let config = config.normalized();
        self.audio.set_volume(config.volume);
        self.cpu.debug = config.debug_logging;
        self.config = config;
    }

    /// Add an enabled cheat using the next available frontend slot.
    pub fn add_cheat(&mut self, code: &str) -> Result<u32, CheatParseError> {
        self.cheats.add_code(code, &self.memory)
    }

    /// Set, disable, replace, or remove a frontend-managed cheat slot.
    pub fn set_cheat_slot(
        &mut self,
        index: u32,
        enabled: bool,
        code: &str,
    ) -> Result<(), CheatParseError> {
        self.cheats.set_slot(index, enabled, code, &self.memory)
    }

    /// Remove all frontend-managed cheat slots.
    pub fn clear_cheats(&mut self) {
        self.cheats.clear();
    }

    /// Execute one frame (30fps)
    pub fn tick(&mut self) {
        self.tick_count += 1;
        self.api.tick_count = self.tick_count;

        if !self.is_running {
            return;
        }

        // Execute CPU instructions for one frame
        // SPMP8000 runs at ~7.37MHz, so at 30fps that's ~245,760 instructions per frame
        let instructions_per_frame = self.header.cpu_freq() / 30;

        for _ in 0..instructions_per_frame {
            if self.exit_requested {
                break;
            }

            let cpu_result = self.cpu.step(&mut self.memory);
            self.api.advance_instructions(1);

            match cpu_result {
                Ok(crate::arm_cpu::CpuResult::Continue) => {}
                Ok(crate::arm_cpu::CpuResult::SvcCall(svc_num)) => {
                    self.sync_cpu_registers_to_memory();
                    let frame_presented = self.api.handle_svc(svc_num, &mut self.memory);
                    self.sync_memory_registers_to_cpu();
                    if frame_presented {
                        self.present_frame();
                    }
                }
                Ok(crate::arm_cpu::CpuResult::Halt) => {
                    self.is_running = false;
                    break;
                }
                Err(e) => {
                    let pc = self.cpu.get_pc().unwrap_or(0);
                    if self.should_skip_cpu_error(&e) {
                        log::warn!(
                            "Skipping invalid instruction at PC=0x{:08X}",
                            pc.wrapping_sub(4)
                        );
                        continue;
                    }
                    log::error!("CPU error at PC=0x{:08X}: {:?}", pc, e);
                    log::error!(
                        "regs: r0=0x{:08X} r1=0x{:08X} r2=0x{:08X} r3=0x{:08X} r4=0x{:08X} r5=0x{:08X} r6=0x{:08X} r7=0x{:08X} r8=0x{:08X} r9=0x{:08X} r10=0x{:08X} r11=0x{:08X} r12=0x{:08X} sp=0x{:08X} lr=0x{:08X}",
                        self.cpu.regs.r0,
                        self.cpu.regs.r1,
                        self.cpu.regs.r2,
                        self.cpu.regs.r3,
                        self.cpu.regs.r4,
                        self.cpu.regs.r5,
                        self.cpu.regs.r6,
                        self.cpu.regs.r7,
                        self.cpu.regs.r8,
                        self.cpu.regs.r9,
                        self.cpu.regs.r10,
                        self.cpu.regs.r11,
                        self.cpu.regs.r12,
                        self.cpu.regs.sp,
                        self.cpu.regs.lr
                    );
                    for addr in pc.saturating_sub(16)..pc.saturating_add(16) {
                        if addr % 4 == 0 {
                            if let Ok(instr) = self.memory.read_u32(addr) {
                                log::error!("code 0x{:08X}: 0x{:08X}", addr, instr);
                            }
                        }
                    }
                    self.is_running = false;
                    break;
                }
                _ => {}
            }
        }

        // Apply enabled freezes after game logic and before frame outputs.
        if cheat_trace_enabled() {
            report_clobbered_cheats(&self.cheats, &self.memory);
        }
        self.cheats.apply(&mut self.memory, &mut self.cpu);

        for command in self.api.take_audio_commands() {
            self.audio.handle_command(command);
        }
        let streamed_pcm = self
            .api
            .audio_buffer_addr
            .map(|address| (address, self.api.audio_buffer_size, self.api.audio_channels));
        self.audio.render_frame(&self.memory, streamed_pcm);
    }

    fn present_frame(&mut self) {
        if self.renderer.fb_addr != self.api.framebuffer_addr {
            self.renderer
                .set_framebuffer_address(self.api.framebuffer_addr);
        }
        if (1..=640).contains(&self.api.framebuffer_width)
            && (1..=480).contains(&self.api.framebuffer_height)
            && (self.renderer.width != self.api.framebuffer_width
                || self.renderer.height != self.api.framebuffer_height)
        {
            self.renderer
                .set_dimensions(self.api.framebuffer_width, self.api.framebuffer_height);
        }
        self.renderer.update_from_memory(&self.memory);
    }

    fn should_skip_cpu_error(&self, error: &crate::arm_cpu::CpuError) -> bool {
        matches!(error, crate::arm_cpu::CpuError::InvalidInstruction(_))
            && self.config.unknown_instruction_policy == UnknownInstructionPolicy::Skip
            && !self.cpu.thumb_mode
    }

    fn sync_cpu_registers_to_memory(&mut self) {
        for reg in 0..=15 {
            let value = self.cpu.regs.get(reg as u32);
            self.memory.set_register(reg, value);
        }
        self.memory
            .set_register(memory::REG_CPSR, self.cpu.regs.cpsr);
    }

    fn sync_memory_registers_to_cpu(&mut self) {
        for reg in 0..=15 {
            self.cpu.regs.set(reg as u32, self.memory.get_register(reg));
        }
        self.cpu.regs.cpsr = self.memory.get_register(memory::REG_CPSR);
    }
    /// Start the emulation
    pub fn start(&mut self) {
        self.is_running = true;
        log::info!("Emulation started");
    }

    /// Stop the emulation
    pub fn stop(&mut self) {
        self.is_running = false;
        log::info!("Emulation stopped");
    }

    /// Get the fixed libretro serialization capacity.
    pub fn serialize_size(&self) -> usize {
        crate::save_state::SERIALIZED_SIZE
    }

    /// Serialize the complete mutable runtime state.
    pub fn serialize(&self, buffer: &mut [u8]) -> Result<()> {
        use crate::save_state::{EmulatorStateRef, MEMORY_LAYOUT_VERSION};

        let state = EmulatorStateRef {
            memory_layout_version: MEMORY_LAYOUT_VERSION,
            cpu: &self.cpu,
            memory: &self.memory,
            api: &self.api,
            renderer: &self.renderer,
            audio: &self.audio,
            input: &self.input,
            tick_count: self.tick_count,
            is_running: self.is_running,
            exit_requested: self.exit_requested,
        };
        crate::save_state::encode(&state, self.content_crc32(), buffer)
    }

    /// Restore a complete runtime state without mutating the current state on failure.
    pub fn deserialize(&mut self, buffer: &[u8]) -> Result<()> {
        use crate::save_state::MEMORY_LAYOUT_VERSION;

        let state = crate::save_state::decode(buffer, self.content_crc32())?;
        if state.memory_layout_version != MEMORY_LAYOUT_VERSION {
            anyhow::bail!(
                "unsupported save-state memory layout {}",
                state.memory_layout_version
            );
        }

        state.memory.validate_state()?;
        state
            .api
            .validate_state(&self.api.game_dir, self.header.cpu_freq())?;
        state.renderer.validate_state()?;
        state.audio.validate_state()?;
        state.input.validate_state()?;
        if state.api.tick_count != state.tick_count {
            anyhow::bail!("save state contains inconsistent tick counters");
        }
        Self::validate_optional_address(
            &state.memory,
            state.api.framebuffer_addr,
            "HLE framebuffer",
        )?;
        Self::validate_optional_address(
            &state.memory,
            state.api.display_screen_addr,
            "HLE display screen",
        )?;
        Self::validate_optional_address(
            &state.memory,
            state.api.audio_buffer_addr,
            "HLE audio buffer",
        )?;
        Self::validate_optional_address(
            &state.memory,
            state.renderer.fb_addr,
            "renderer framebuffer",
        )?;

        self.memory.copy_state_from(&state.memory)?;
        let stable_memory = std::mem::replace(&mut self.memory, state.memory);
        let mut replacement = Self {
            header: self.header.clone(),
            game_path: self.game_path.clone(),
            boot_code: Arc::clone(&self.boot_code),
            cpu: state.cpu,
            memory: stable_memory,
            function_table: FunctionTable::new(),
            api: state.api,
            renderer: state.renderer,
            audio: state.audio,
            input: state.input,
            cheats: self.cheats.clone(),
            config: self.config.clone(),
            tick_count: state.tick_count,
            is_running: state.is_running,
            exit_requested: state.exit_requested,
        };
        replacement.set_config(self.config.clone());
        *self = replacement;
        log::info!("Emulator state restored");
        Ok(())
    }

    fn content_crc32(&self) -> u32 {
        crc32fast::hash(&self.boot_code)
    }

    fn validate_optional_address(
        memory: &Memory,
        address: Option<u32>,
        description: &str,
    ) -> Result<()> {
        if let Some(address) = address {
            memory
                .read_u8(address)
                .with_context(|| format!("save state contains an invalid {description} address"))?;
        }
        Ok(())
    }

    /// Rebuild all mutable runtime state from the cached boot image.
    pub fn reset(&mut self) -> Result<()> {
        let config = self.config.clone();
        let input = self.input.clone();

        let mut replacement = Self::from_loaded_game(
            self.game_path.clone(),
            self.header.clone(),
            Arc::clone(&self.boot_code),
            config,
            input,
        )
        .context("Failed to rebuild emulator state")?;
        self.memory.copy_state_from(&replacement.memory)?;
        std::mem::swap(&mut self.memory, &mut replacement.memory);
        replacement.cheats = self.cheats.clone();
        *self = replacement;
        log::info!("Emulator reset");
        Ok(())
    }

    /// Request exit
    pub fn request_exit(&mut self) {
        self.exit_requested = true;
    }

    /// Check if exit was requested
    pub fn should_exit(&self) -> bool {
        self.exit_requested
    }

    /// Get the framebuffer data
    pub fn get_framebuffer(&self) -> &[u8] {
        self.renderer.get_framebuffer()
    }

    /// Get the audio buffer
    pub fn get_audio_samples(&self) -> &[i16] {
        self.audio.get_buffer()
    }

    /// Get tick count
    pub fn get_tick_count(&self) -> u64 {
        self.tick_count
    }

    /// Check if emulation is running
    pub fn is_running(&self) -> bool {
        self.is_running
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{FileHandle, Surface};
    use crate::audio_resource::{AudioCommand, RESOURCE_TYPE_WAV};
    use crate::bin_loader::ChipType;
    use crate::input_handler::{Button, BUTTON_O};
    use crate::memory::{FUNC_TABLE_BASE, PERIPHERAL_BASE, RAM_BASE, REG_R5, VRAM_BASE};
    use crate::renderer::PixelFormat;

    fn test_emulator(volume: u32) -> Emulator {
        test_emulator_with_code(volume, &[0x01, 0x02, 0x03, 0x04])
    }

    fn test_emulator_with_code(volume: u32, boot_code: &[u8]) -> Emulator {
        let header = NGameHeader {
            magic: *b"NGame1.0",
            flags: 0x8000_0000,
            vendor: "Sunplus".to_string(),
            chip_type: ChipType::SPMP8000,
            game_name: "Reset Test".to_string(),
            media_type: "Sunmedia".to_string(),
            version: "1.0".to_string(),
            code_size: boot_code.len() as u32,
            file_size: 0x84,
            data_offset: 0x80,
        };

        Emulator::from_loaded_game(
            PathBuf::from("games/reset-test.bin"),
            header,
            Arc::from(boot_code),
            CoreConfig {
                volume,
                ..CoreConfig::default()
            },
            InputHandler::new(),
        )
        .unwrap()
    }

    fn wave_resource() -> Vec<u8> {
        let samples = [0u8, 255, 0, 255];
        let mut wave = Vec::new();
        wave.extend_from_slice(b"RIFF");
        wave.extend_from_slice(&(36 + samples.len() as u32).to_le_bytes());
        wave.extend_from_slice(b"WAVEfmt ");
        wave.extend_from_slice(&16u32.to_le_bytes());
        wave.extend_from_slice(&1u16.to_le_bytes());
        wave.extend_from_slice(&1u16.to_le_bytes());
        wave.extend_from_slice(&22_050u32.to_le_bytes());
        wave.extend_from_slice(&22_050u32.to_le_bytes());
        wave.extend_from_slice(&1u16.to_le_bytes());
        wave.extend_from_slice(&8u16.to_le_bytes());
        wave.extend_from_slice(b"data");
        wave.extend_from_slice(&(samples.len() as u32).to_le_bytes());
        wave.extend_from_slice(&samples);
        wave
    }

    #[test]
    fn test_emulator_creation() {
        // This test requires a valid BIN file
        // Skip if test file doesn't exist
        let test_path = PathBuf::from("test_data/test.bin");
        if !test_path.exists() {
            return;
        }

        let emu = Emulator::from_path(test_path, 100);
        assert!(emu.is_ok());
    }

    #[test]
    fn framebuffer_is_sampled_when_the_game_presents_it() {
        let words = [
            0xE59F_1014, // LDR r1, [pc, #20]
            0xE59F_2014, // LDR r2, [pc, #20]
            0xE581_2000, // STR r2, [r1]
            0xEF00_0003, // SVC MCatchPaint
            0xE59F_200C, // LDR r2, [pc, #12]
            0xE581_2000, // STR r2, [r1]
            0xEAFF_FFFE, // B .
            VRAM_BASE,
            0x0000_F800,
            0x0000_07E0,
        ];
        let boot_code = words
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect::<Vec<_>>();
        let mut emu = test_emulator_with_code(0, &boot_code);

        emu.start();
        emu.tick();

        assert_eq!(emu.memory.read_u16(VRAM_BASE).unwrap(), 0x07E0);
        // Little-endian XRGB8888: [B, G, R, X] — 0xF800 is pure red in RGB565
        assert_eq!(&emu.get_framebuffer()[..4], &[0, 0, 255, 0]);
    }

    #[test]
    fn reset_rebuilds_runtime_state_and_preserves_configuration() {
        let mut emu = test_emulator(37);
        let initial_trampoline = emu.memory.read_u32(FUNC_TABLE_BASE).unwrap();
        let ram_pointer = emu.memory.system_ram_mut().unwrap().as_mut_ptr();
        let vram_pointer = emu.memory.video_ram_mut().unwrap().as_mut_ptr();
        let peripheral_pointer = emu.memory.peripheral_memory_mut().unwrap().as_mut_ptr();
        emu.set_cheat_slot(4, true, "mem8:0x1000=99").unwrap();

        emu.set_buttons(1 << BUTTON_O);
        emu.set_config(CoreConfig {
            volume: 37,
            debug_logging: true,
            ..CoreConfig::default()
        });
        emu.cpu.thumb_mode = true;
        emu.cpu.instruction_count = 123;
        emu.cpu.set_register(4, 0xDEAD_BEEF).unwrap();
        emu.memory
            .write_u32(RAM_BASE + 0x1000, 0x1111_1111)
            .unwrap();
        emu.memory.write_u32(VRAM_BASE, 0x2222_2222).unwrap();
        emu.memory.write_u32(PERIPHERAL_BASE, 0x3333_3333).unwrap();
        emu.memory.write_u32(memory::CODE_LOAD_ADDR, 0).unwrap();
        emu.memory.write_u32(FUNC_TABLE_BASE, 0).unwrap();
        emu.api.framebuffer_addr = Some(VRAM_BASE);
        emu.api.audio_buffer_addr = Some(RAM_BASE + 0x2000);
        emu.api.resource_table.push(("sound".to_string(), 0x1234));
        emu.api.open_files.insert(
            3,
            FileHandle {
                host_path: "save.dat".to_string(),
                position: 4,
                size: 8,
                is_writable: true,
            },
        );
        emu.api.next_fd = 4;
        emu.renderer.set_dimensions(160, 120);
        emu.renderer.set_framebuffer_address(Some(VRAM_BASE));
        emu.renderer.get_framebuffer_mut().fill(0xFF);
        emu.audio.set_params(44100, 1);
        emu.audio.get_buffer_mut().extend_from_slice(&[1, 2, 3, 4]);
        emu.tick_count = 42;
        emu.api.tick_count = 42;
        emu.request_exit();
        emu.start();

        emu.reset().unwrap();

        assert!(!emu.is_running());
        assert!(!emu.should_exit());
        assert_eq!(emu.get_tick_count(), 0);
        assert_eq!(emu.cpu.instruction_count, 0);
        assert!(!emu.cpu.thumb_mode);
        assert!(emu.cpu.debug);
        assert_eq!(emu.cpu.get_pc().unwrap(), memory::CODE_LOAD_ADDR);
        assert_eq!(emu.cpu.regs.sp, 0x00F0_0000);
        assert_eq!(emu.cpu.regs.r0, FUNC_TABLE_BASE);
        assert_eq!(
            emu.memory.read_u32(memory::CODE_LOAD_ADDR).unwrap(),
            0x0403_0201
        );
        assert_eq!(
            emu.memory.read_u32(FUNC_TABLE_BASE).unwrap(),
            initial_trampoline
        );
        assert_eq!(emu.memory.read_u32(RAM_BASE + 0x1000).unwrap(), 0);
        assert_eq!(emu.memory.read_u32(VRAM_BASE).unwrap(), 0);
        assert_eq!(emu.memory.read_u32(PERIPHERAL_BASE).unwrap(), 0);
        assert_eq!(emu.api.framebuffer_addr, None);
        assert_eq!(emu.api.audio_buffer_addr, None);
        assert!(emu.api.resource_table.is_empty());
        assert!(emu.api.open_files.is_empty());
        assert_eq!(emu.api.next_fd, 3);
        assert_eq!(emu.api.game_dir, "games");
        assert_eq!(emu.api.get_key_state(), 0);
        assert_eq!(emu.renderer.width, 320);
        assert_eq!(emu.renderer.height, 240);
        assert_eq!(emu.renderer.fb_addr, None);
        assert!(emu.renderer.get_framebuffer().iter().all(|byte| *byte == 0));
        assert_eq!(emu.audio.sample_rate, 22050);
        assert_eq!(emu.audio.channels, 2);
        assert_eq!(emu.audio.get_volume(), 37);
        assert!(emu.audio.get_buffer().is_empty());
        assert_eq!(emu.input.get_buttons(), 0);
        assert_eq!(emu.cheats.get_slot(4).unwrap().code, "mem8:0x1000=99");
        assert_eq!(emu.config.volume, 37);
        assert!(emu.config.debug_logging);
        assert_eq!(
            emu.memory.system_ram_mut().unwrap().as_mut_ptr(),
            ram_pointer
        );
        assert_eq!(
            emu.memory.video_ram_mut().unwrap().as_mut_ptr(),
            vram_pointer
        );
        assert_eq!(
            emu.memory.peripheral_memory_mut().unwrap().as_mut_ptr(),
            peripheral_pointer
        );
    }

    #[test]
    fn reset_uses_the_cached_boot_image_without_rereading_content() {
        let mut emu = test_emulator(100);
        assert!(!emu.game_path.exists());
        emu.memory.write_u32(memory::CODE_LOAD_ADDR, 0).unwrap();

        emu.reset().unwrap();

        assert_eq!(
            emu.memory.read_u32(memory::CODE_LOAD_ADDR).unwrap(),
            0x0403_0201
        );
    }

    #[test]
    fn save_state_round_trip_restores_the_complete_runtime() {
        let mut emu = test_emulator(37);
        emu.cpu.thumb_mode = true;
        emu.cpu.instruction_count = 123_456;
        emu.cpu.regs.r4 = 0xDEAD_BEEF;
        emu.cpu.regs.cpsr = 0xA000_001F;
        emu.memory
            .write_u32(RAM_BASE + 0x1000, 0x1111_1111)
            .unwrap();
        emu.memory.write_u32(VRAM_BASE, 0x2222_2222).unwrap();
        emu.memory.write_u32(PERIPHERAL_BASE, 0x3333_3333).unwrap();
        emu.memory.set_register(REG_R5, 0xCAFE_BABE);

        emu.api.framebuffer_addr = Some(VRAM_BASE);
        emu.api.display_screen_addr = Some(VRAM_BASE + 0x100);
        emu.api.framebuffer_width = 160;
        emu.api.framebuffer_height = 120;
        emu.api.framebuffer_pitch = 320;
        emu.api.fg_color = [1, 2, 3];
        emu.api.color_rop = 0xCC;
        emu.api.surfaces.insert(
            7,
            Surface {
                data_addr: RAM_BASE + 0x4000,
                width: 16,
                height: 8,
                img_type: 1,
                palette_addr: RAM_BASE + 0x5000,
                palette_entries: 16,
            },
        );
        emu.api.next_surface_id = 8;
        emu.api.audio_buffer_size = 128;
        emu.api.audio_sample_rate = 44_100;
        emu.api.audio_channels = 2;
        emu.api.raw_key_state = 0x12;
        emu.api.key_state = 0x34;
        emu.api.key_map[0] = 0x56;
        emu.api.open_files.insert(
            3,
            FileHandle {
                host_path: "games/save.dat".to_string(),
                position: 4,
                size: 8,
                is_writable: true,
            },
        );
        emu.api.next_fd = 4;
        emu.api.start_time = 99;
        emu.api.tick_count = 77;
        emu.api.resource_table.push(("sound".to_string(), 0x1234));
        emu.api
            .audio_commands
            .push(AudioCommand::Stop { resource_type: 99 });
        emu.api.advance_instructions(555);

        emu.renderer.set_dimensions(160, 120);
        emu.renderer.set_format(PixelFormat::XRGB8888);
        emu.renderer.set_framebuffer_address(Some(VRAM_BASE));
        emu.renderer.get_framebuffer_mut().fill(0x5A);

        emu.audio.set_params(44_100, 2);
        emu.audio.set_volume(37);
        emu.audio.get_buffer_mut().extend_from_slice(&[1, 2, 3, 4]);
        emu.audio.handle_command(AudioCommand::Play {
            resource_type: RESOURCE_TYPE_WAV,
            repeat: 2,
            data: wave_resource(),
        });

        emu.input.set_buttons(1 << BUTTON_O);
        emu.tick_count = 77;
        emu.is_running = true;
        emu.exit_requested = true;

        let mut expected_audio = emu.audio.clone();
        expected_audio.render_frame(&emu.memory, None);
        let expected_next_audio = expected_audio.get_buffer().to_vec();
        let expected_framebuffer = emu.renderer.get_framebuffer().to_vec();

        let mut state = vec![0u8; emu.serialize_size()];
        emu.serialize(&mut state).unwrap();
        emu.reset().unwrap();
        emu.deserialize(&state).unwrap();

        assert!(emu.cpu.thumb_mode);
        assert_eq!(emu.cpu.instruction_count, 123_456);
        assert_eq!(emu.cpu.regs.r4, 0xDEAD_BEEF);
        assert_eq!(emu.cpu.regs.cpsr, 0xA000_001F);
        assert_eq!(emu.memory.read_u32(RAM_BASE + 0x1000).unwrap(), 0x1111_1111);
        assert_eq!(emu.memory.read_u32(VRAM_BASE).unwrap(), 0x2222_2222);
        assert_eq!(emu.memory.read_u32(PERIPHERAL_BASE).unwrap(), 0x3333_3333);
        assert_eq!(emu.memory.get_register(REG_R5), 0xCAFE_BABE);
        assert_eq!(emu.api.framebuffer_addr, Some(VRAM_BASE));
        assert_eq!(emu.api.display_screen_addr, Some(VRAM_BASE + 0x100));
        assert_eq!(emu.api.framebuffer_width, 160);
        assert_eq!(emu.api.framebuffer_height, 120);
        assert_eq!(emu.api.fg_color, [1, 2, 3]);
        assert_eq!(emu.api.color_rop, 0xCC);
        assert_eq!(emu.api.surfaces.get(&7).unwrap().width, 16);
        assert_eq!(emu.api.next_surface_id, 8);
        assert_eq!(emu.api.raw_key_state, 0x12);
        assert_eq!(emu.api.key_state, 0x34);
        assert_eq!(emu.api.key_map[0], 0x56);
        assert_eq!(emu.api.open_files.get(&3).unwrap().position, 4);
        assert_eq!(emu.api.next_fd, 4);
        assert_eq!(emu.api.start_time, 99);
        assert_eq!(emu.api.tick_count, 77);
        assert_eq!(emu.api.resource_table, [("sound".to_string(), 0x1234)]);
        assert_eq!(emu.renderer.width, 160);
        assert_eq!(emu.renderer.height, 120);
        assert_eq!(emu.renderer.format, PixelFormat::XRGB8888);
        assert_eq!(emu.renderer.fb_addr, Some(VRAM_BASE));
        assert_eq!(emu.renderer.get_framebuffer(), expected_framebuffer);
        assert_eq!(emu.audio.sample_rate, 44_100);
        assert_eq!(emu.audio.channels, 2);
        assert_eq!(emu.audio.get_volume(), 37);
        assert_eq!(emu.audio.get_buffer(), [1, 2, 3, 4]);
        emu.audio.render_frame(&emu.memory, None);
        assert_eq!(emu.audio.get_buffer(), expected_next_audio);
        assert_eq!(emu.input.get_buttons(), 1 << BUTTON_O);
        assert_eq!(emu.tick_count, 77);
        assert!(emu.is_running);
        assert!(emu.exit_requested);
        assert!(matches!(
            emu.api.take_audio_commands().as_slice(),
            [AudioCommand::Stop { resource_type: 99 }]
        ));
    }

    #[test]
    fn save_state_rejects_corruption_and_different_content_transactionally() {
        let source = test_emulator(100);
        let mut state = vec![0u8; source.serialize_size()];
        source.serialize(&mut state).unwrap();

        let mut loaded_target = test_emulator(100);
        loaded_target.set_config(CoreConfig {
            volume: 40,
            swap_o_x: true,
            debug_logging: true,
            unknown_instruction_policy: UnknownInstructionPolicy::Skip,
        });
        loaded_target.start();
        loaded_target.cpu.regs.r4 = 1;
        loaded_target.deserialize(&state).unwrap();
        assert!(!loaded_target.is_running());
        assert_eq!(loaded_target.cpu.regs.r4, 0);
        assert_eq!(loaded_target.tick_count, 0);
        assert_eq!(loaded_target.config.volume, 40);
        assert!(loaded_target.config.swap_o_x);
        assert!(loaded_target.config.debug_logging);
        assert_eq!(loaded_target.audio.get_volume(), 40);
        assert!(loaded_target.cpu.debug);

        let mut target = test_emulator(100);
        target.cpu.regs.r4 = 0x1234_5678;
        state[32] ^= 1;
        assert!(target.deserialize(&state).is_err());
        assert_eq!(target.cpu.regs.r4, 0x1234_5678);

        source.serialize(&mut state).unwrap();
        let mut different = test_emulator_with_code(100, &[5, 6, 7, 8]);
        different.cpu.regs.r4 = 0x8765_4321;
        assert!(different.deserialize(&state).is_err());
        assert_eq!(different.cpu.regs.r4, 0x8765_4321);
    }

    #[test]
    fn save_state_keeps_memory_pointers_and_current_cheat_configuration() {
        let mut emu = test_emulator(100);
        let address = RAM_BASE + 0x2000;
        emu.memory.write_u32(address, 7).unwrap();
        emu.set_cheat_slot(0, true, "mem32:0x00002000=11").unwrap();
        let mut state = vec![0u8; emu.serialize_size()];
        emu.serialize(&mut state).unwrap();

        emu.set_cheat_slot(0, true, "mem32:0x00002000=22").unwrap();
        emu.memory.write_u32(address, 99).unwrap();
        let ram_pointer = emu.memory.system_ram_mut().unwrap().as_mut_ptr();
        let vram_pointer = emu.memory.video_ram_mut().unwrap().as_mut_ptr();
        let peripheral_pointer = emu.memory.peripheral_memory_mut().unwrap().as_mut_ptr();

        emu.deserialize(&state).unwrap();

        assert_eq!(emu.memory.read_u32(address).unwrap(), 7);
        assert_eq!(emu.cheats.get_slot(0).unwrap().code, "mem32:0x00002000=22");
        assert_eq!(
            emu.memory.system_ram_mut().unwrap().as_mut_ptr(),
            ram_pointer
        );
        assert_eq!(
            emu.memory.video_ram_mut().unwrap().as_mut_ptr(),
            vram_pointer
        );
        assert_eq!(
            emu.memory.peripheral_memory_mut().unwrap().as_mut_ptr(),
            peripheral_pointer
        );

        emu.cheats.apply(&mut emu.memory, &mut emu.cpu);
        assert_eq!(emu.memory.read_u32(address).unwrap(), 22);
    }

    #[test]
    fn enabled_cheats_are_applied_at_the_end_of_a_running_frame() {
        let mut emu = test_emulator(100);
        emu.set_cheat_slot(0, true, "mem8:0x1000=99").unwrap();
        emu.set_cheat_slot(1, true, "reg:r4=0x12345678").unwrap();
        emu.start();

        emu.tick();

        assert_eq!(emu.memory.read_u8(RAM_BASE + 0x1000).unwrap(), 99);
        assert_eq!(emu.cpu.regs.r4, 0x1234_5678);
    }

    #[test]
    fn live_config_updates_outputs_without_resetting_runtime() {
        let mut emu = test_emulator(100);
        emu.start();
        emu.tick_count = 12;
        emu.cpu.regs.r4 = 0x1234_5678;
        emu.memory
            .write_u32(RAM_BASE + 0x1000, 0xABCD_EF01)
            .unwrap();

        emu.set_config(CoreConfig {
            volume: 35,
            swap_o_x: true,
            debug_logging: true,
            unknown_instruction_policy: UnknownInstructionPolicy::Skip,
        });
        emu.set_buttons(1 << BUTTON_O);

        assert_eq!(emu.audio.get_volume(), 35);
        assert!(emu.cpu.debug);
        assert_eq!(emu.input.get_buttons(), 1 << crate::input_handler::BUTTON_X);
        assert_eq!(emu.api.raw_key_state, 1 << crate::input_handler::BUTTON_X);
        assert_eq!(emu.tick_count, 12);
        assert!(emu.is_running);
        assert_eq!(emu.cpu.regs.r4, 0x1234_5678);
        assert_eq!(emu.memory.read_u32(RAM_BASE + 0x1000).unwrap(), 0xABCD_EF01);
    }

    #[test]
    fn frontend_input_is_limited_to_supported_logical_buttons() {
        let mut emu = test_emulator(100);
        emu.set_buttons(Button::O.mask() | (1 << 31));
        assert_eq!(emu.input.get_buttons(), Button::O.mask());
        assert_eq!(emu.api.raw_key_state, Button::O.mask());
    }

    #[test]
    fn unknown_instruction_policy_only_skips_invalid_arm_instructions() {
        let mut emu = test_emulator(100);
        let invalid = crate::arm_cpu::CpuError::InvalidInstruction(0xFFFF_FFFF);

        assert!(!emu.should_skip_cpu_error(&invalid));

        emu.set_config(CoreConfig {
            unknown_instruction_policy: UnknownInstructionPolicy::Skip,
            ..CoreConfig::default()
        });
        assert!(emu.should_skip_cpu_error(&invalid));
        assert!(
            !emu.should_skip_cpu_error(&crate::arm_cpu::CpuError::MemoryError("test".to_string()))
        );

        emu.cpu.thumb_mode = true;
        assert!(!emu.should_skip_cpu_error(&invalid));
    }
}
