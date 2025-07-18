// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use super::chip::HlComms;
use super::error::PlatformError;

#[derive(Debug, Clone, Copy)]
pub struct ArcMsgAddr {
    pub scratch_base: u32,
    pub arc_misc_cntl: u32,
}

#[derive(Debug)]
pub enum TypedArcMsg {
    GetSmbusTelemetryAddr,
}

impl TypedArcMsg {
    pub fn msg_code(&self) -> u16 {
        match self {
            TypedArcMsg::GetSmbusTelemetryAddr => 0x2C,
        }
    }
}

#[derive(Debug)]
pub enum ArcMsg {
    Typed(TypedArcMsg),
}

#[derive(Debug)]
pub enum ArcMsgOk {
    Ok { arg: u32 },
    OkNoWait,
}

pub struct ArcMsgOptions {
    pub msg: ArcMsg,
    pub wait_for_done: bool,
    pub timeout: std::time::Duration,
    pub use_second_mailbox: bool,
    pub addrs: Option<ArcMsgAddr>,
}

impl Default for ArcMsgOptions {
    fn default() -> Self {
        Self {
            msg: ArcMsg::Typed(TypedArcMsg::GetSmbusTelemetryAddr),
            wait_for_done: true,
            timeout: std::time::Duration::from_secs(1),
            use_second_mailbox: false,
            addrs: None,
        }
    }
}

/// Send an ARC message and wait for response
pub fn arc_msg<T: HlComms>(
    chip: &T,
    msg: &ArcMsg,
    wait_for_done: bool,
    timeout: std::time::Duration,
    msg_reg: u64,
    return_reg: u64,
    addrs: &ArcMsgAddr,
) -> Result<ArcMsgOk, PlatformError> {
    let (msg_code, arg0, _arg1) = match msg {
        ArcMsg::Typed(typed_msg) => {
            let msg_code = typed_msg.msg_code();
            (msg_code, 0u16, 0u16)
        }
    };

    let (_comms, ifc) = chip.comms_obj();

    // Write the message code with the 0xaa00 magic prefix
    let msg_val = 0xaa00 | msg_code as u32;

    // Write args to return register first if needed
    ifc.axi_write(
        addrs.scratch_base + (return_reg as u32 * 4),
        &(arg0 as u32).to_le_bytes(),
    )?;

    // Write the message
    ifc.axi_write(
        addrs.scratch_base + (msg_reg as u32 * 4),
        &msg_val.to_le_bytes(),
    )?;

    // Trigger the interrupt by setting bit 16
    let misc_val = ifc.axi_read32(addrs.arc_misc_cntl)?;
    ifc.axi_write(addrs.arc_misc_cntl, &(misc_val | (1 << 16)).to_le_bytes())?;

    if !wait_for_done {
        return Ok(ArcMsgOk::OkNoWait);
    }

    // Wait for response
    let start = std::time::Instant::now();
    loop {
        let status = ifc.axi_read32(addrs.scratch_base + (msg_reg as u32 * 4))?;

        // Check if message is complete - the lower 16 bits should match our message code
        if (status & 0xFFFF) as u16 == msg_code {
            let _exit_code = (status >> 16) & 0xFFFF;
            let arg = ifc.axi_read32(addrs.scratch_base + (return_reg as u32 * 4))?;
            return Ok(ArcMsgOk::Ok { arg });
        }

        // Check for error response
        if status == 0xffffffff {
            return Err(PlatformError::Generic(
                format!("ARC message not recognized: 0x{msg_code:04x}"),
                super::error::BtWrapper::capture(),
            ));
        }

        if start.elapsed() > timeout {
            return Err(PlatformError::Generic(
                "ARC message timeout".to_string(),
                super::error::BtWrapper::capture(),
            ));
        }

        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
