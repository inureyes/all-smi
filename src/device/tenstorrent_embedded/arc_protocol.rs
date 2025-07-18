// SPDX-FileCopyrightText: © 2025 All-SMI Contributors
// SPDX-License-Identifier: Apache-2.0

//! ARC (Argonaut RISC Core) message protocol implementation
//! Based on TT-REPORT.md specifications

use super::error::PlatformError;
use std::time::{Duration, Instant};

/// ARC message header magic value
const ARC_MSG_MAGIC: u16 = 0xaa55;

/// ARC response magic value
const ARC_RESPONSE_MAGIC: u16 = 0x55aa;

/// Default timeout for ARC messages
const ARC_MSG_TIMEOUT: Duration = Duration::from_secs(1);

/// ARC scratch register offsets
pub struct ArcRegisters;

impl ArcRegisters {
    /// Translate symbolic register names to addresses
    pub fn translate(reg_name: &str) -> Result<u64, PlatformError> {
        match reg_name {
            // ARC reset registers
            "ARC_RESET.SCRATCH[0]" => Ok(0x1FF30060),
            "ARC_RESET.SCRATCH[1]" => Ok(0x1FF30064),
            "ARC_RESET.SCRATCH[2]" => Ok(0x1FF30068),
            "ARC_RESET.ARC_MISC_CNTL" => Ok(0x1FF30100),

            // CSM (Code Storage Memory) registers
            "ARC_CSM.DATA[0]" => Ok(0x1FEF0000),

            // Blackhole specific
            "arc_ss.reset_unit.SCRATCH_0" => Ok(0xFFB2A060),
            "arc_ss.reset_unit.SCRATCH_1" => Ok(0xFFB2A064),
            "arc_ss.reset_unit.SCRATCH_2" => Ok(0xFFB2A068),
            "arc_ss.reset_unit.ARC_MISC_CNTL" => Ok(0xFFB2A100),

            _ => Err(PlatformError::InvalidParameter(format!(
                "Unknown register: {reg_name}"
            ))),
        }
    }
}

/// ARC message structure
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ArcMsg {
    pub msg_type: u16,
    pub msg_code: u8,
    pub return_code: u8,
    pub arg0: u32,
    pub arg1: u32,
}

impl ArcMsg {
    /// Create a new ARC message
    pub fn new(msg_code: TypedArcMsg) -> Self {
        Self {
            msg_type: ARC_MSG_MAGIC,
            msg_code: msg_code as u8,
            return_code: 0,
            arg0: 0,
            arg1: 0,
        }
    }

    /// Convert to bytes for register writes
    pub fn to_bytes(&self) -> [u8; 12] {
        let mut bytes = [0u8; 12];
        bytes[0..2].copy_from_slice(&self.msg_type.to_le_bytes());
        bytes[2] = self.msg_code;
        bytes[3] = self.return_code;
        bytes[4..8].copy_from_slice(&self.arg0.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.arg1.to_le_bytes());
        bytes
    }

    /// Parse from register values
    pub fn from_registers(reg0: u32, reg1: u32, reg2: u32) -> Self {
        Self {
            msg_type: (reg0 & 0xFFFF) as u16,
            msg_code: ((reg0 >> 16) & 0xFF) as u8,
            return_code: ((reg0 >> 24) & 0xFF) as u8,
            arg0: reg1,
            arg1: reg2,
        }
    }
}

/// Typed ARC message codes
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum TypedArcMsg {
    Nop = 0x11,
    GetSmbusTelemetryAddr = 0x2C,
    GetEthSmbusTelemetryAddr = 0x2D,
    // Add more message types as needed
}

/// ARC message handler trait
pub trait ArcMessageHandler {
    /// Read a 32-bit value from an AXI address
    fn axi_read32(&self, addr: u64) -> Result<u32, PlatformError>;

    /// Write a 32-bit value to an AXI address
    fn axi_write32(&self, addr: u64, value: u32) -> Result<(), PlatformError>;

    /// Get the architecture-specific scratch register base
    fn get_scratch_base(&self) -> Result<&'static str, PlatformError>;

    /// Get the architecture-specific misc control register
    fn get_misc_cntl(&self) -> Result<&'static str, PlatformError>;
}

/// ARC message protocol implementation
pub struct ArcProtocol;

impl ArcProtocol {
    /// Send an ARC message and wait for response
    pub fn send_message<H: ArcMessageHandler>(
        handler: &H,
        msg: TypedArcMsg,
    ) -> Result<u32, PlatformError> {
        Self::send_message_with_args(handler, msg, 0, 0)
    }

    /// Send an ARC message with arguments and wait for response
    pub fn send_message_with_args<H: ArcMessageHandler>(
        handler: &H,
        msg_code: TypedArcMsg,
        arg0: u32,
        arg1: u32,
    ) -> Result<u32, PlatformError> {
        // Create message
        let mut arc_msg = ArcMsg::new(msg_code);
        arc_msg.arg0 = arg0;
        arc_msg.arg1 = arg1;

        // Get register addresses
        let scratch_base = ArcRegisters::translate(handler.get_scratch_base()?)?;
        let misc_cntl = ArcRegisters::translate(handler.get_misc_cntl()?)?;

        eprintln!("[DEBUG] Sending ARC message {msg_code:?} to scratch base 0x{scratch_base:x}");

        // Convert message to bytes
        let msg_bytes = arc_msg.to_bytes();

        // Write message to scratch registers
        let reg0 = u32::from_le_bytes([msg_bytes[0], msg_bytes[1], msg_bytes[2], msg_bytes[3]]);
        let reg1 = u32::from_le_bytes([msg_bytes[4], msg_bytes[5], msg_bytes[6], msg_bytes[7]]);
        let reg2 = u32::from_le_bytes([msg_bytes[8], msg_bytes[9], msg_bytes[10], msg_bytes[11]]);

        handler.axi_write32(scratch_base, reg0)?;
        handler.axi_write32(scratch_base + 4, reg1)?;
        handler.axi_write32(scratch_base + 8, reg2)?;

        // Trigger doorbell (set bit 5 of misc control register)
        handler.axi_write32(misc_cntl, 1 << 5)?;

        // Wait for response
        let start = Instant::now();
        loop {
            let response_reg0 = handler.axi_read32(scratch_base)?;
            let msg_type = (response_reg0 & 0xFFFF) as u16;

            if msg_type == ARC_RESPONSE_MAGIC {
                // Response received
                let return_code = ((response_reg0 >> 24) & 0xFF) as u8;

                if return_code != 0 {
                    return Err(PlatformError::ChipError(format!(
                        "ARC message failed with return code: {return_code}"
                    )));
                }

                // Read result from arg0 register
                let result = handler.axi_read32(scratch_base + 4)?;
                eprintln!("[DEBUG] ARC message response: 0x{result:x}");

                return Ok(result);
            }

            if start.elapsed() > ARC_MSG_TIMEOUT {
                return Err(PlatformError::Timeout("ARC message timeout".to_string()));
            }

            // Small delay to avoid hammering the bus
            std::thread::sleep(Duration::from_micros(100));
        }
    }

    /// Wait for ARC firmware to be ready
    pub fn wait_for_arc_ready<H: ArcMessageHandler>(
        handler: &H,
        timeout: Duration,
    ) -> Result<(), PlatformError> {
        eprintln!("[DEBUG] Waiting for ARC firmware to be ready...");

        let start = Instant::now();

        // Try sending a NOP message to verify ARC is responsive
        loop {
            match Self::send_message(handler, TypedArcMsg::Nop) {
                Ok(_) => {
                    eprintln!("[DEBUG] ARC firmware is ready");
                    return Ok(());
                }
                Err(_) if start.elapsed() < timeout => {
                    // Continue trying
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(e) => {
                    return Err(PlatformError::ChipError(format!(
                        "ARC firmware not ready after {timeout:?}: {e}"
                    )));
                }
            }
        }
    }

    /// Get telemetry address from ARC
    pub fn get_telemetry_addr<H: ArcMessageHandler>(handler: &H) -> Result<u32, PlatformError> {
        eprintln!("[DEBUG] Getting telemetry address from ARC...");

        let addr = Self::send_message(handler, TypedArcMsg::GetSmbusTelemetryAddr)?;

        eprintln!("[DEBUG] Telemetry address: 0x{addr:x}");
        Ok(addr)
    }
}
