// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0
// Simplified detection logic extracted from luwen-ref

use super::{
    arch::Arch,
    chip::{Chip, ChipComms},
    error::PlatformError,
    luwen_ref::{comms_callback, ExtendedPciDevice},
    ttkmd::kmdif::PciDevice,
};

/// Options for chip detection
#[derive(Default)]
pub struct ChipDetectOptions {
    /// If true, then we will search for chips directly available over a physical interface
    #[allow(dead_code)]
    pub local_only: bool,
    /// If len > 0 then only chips with the given archs will be returned
    pub chip_filter: Vec<Arch>,
}

/// Represents a chip which may or may not be initialized
pub enum UninitChip {
    /// The chip is fine and can be safely used
    Initialized(Chip),
    Partially {
        status: Box<String>,
        underlying: Chip,
    },
}

impl UninitChip {
    /// Initialize the chip
    pub fn init<E>(self, _callback: &mut impl FnMut(()) -> Result<(), E>) -> Result<Chip, E> {
        match self {
            UninitChip::Initialized(chip) => Ok(chip),
            UninitChip::Partially { underlying, .. } => Ok(underlying),
        }
    }
}

/// Error type for detection
#[derive(Debug)]
pub struct DetectError(pub String);

impl std::fmt::Display for DetectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for DetectError {}

pub fn detect_chips_silent(_options: ChipDetectOptions) -> Result<Vec<UninitChip>, PlatformError> {
    let mut chips = Vec::new();

    let device_ids = PciDevice::scan();
    eprintln!(
        "[DEBUG] detect_chips_silent: Found {} device IDs from scan",
        device_ids.len()
    );

    for device_id in device_ids {
        eprintln!("[DEBUG] Attempting to open device ID: {}", device_id);
        let ud = match ExtendedPciDevice::open(device_id) {
            Ok(ud) => ud,
            Err(e) => {
                eprintln!("[DEBUG] Failed to open device {}: {}", device_id, e);
                return Err(e.into());
            }
        };

        let arch = ud.borrow().device.arch;

        let chip = Chip::open(
            arch,
            crate::device::tenstorrent_embedded::interface::CallbackStorage::new(
                comms_callback,
                ud.clone(),
            ),
        )?;

        // First let's test basic pcie communication we may be in a hang state so it's
        // important that we let the detect function know

        // Hack(drosen): Basic init procedure should resolve this
        let scratch_0 = if chip.get_arch().is_blackhole() {
            "arc_ss.reset_unit.SCRATCH_0"
        } else {
            "ARC_RESET.SCRATCH[0]"
        };
        let result = chip.axi_sread32(scratch_0);
        if let Err(err) = result {
            // Basic comms have failed... we should output a nice error message on the console
            chips.push(UninitChip::Partially {
                status: Box::new(err.to_string()),
                underlying: chip,
            });
        } else {
            chips.push(UninitChip::Initialized(chip));
        }
    }

    Ok(chips)
}
