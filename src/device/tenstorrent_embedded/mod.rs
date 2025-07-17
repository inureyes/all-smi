// SPDX-FileCopyrightText: © 2025 Extracted from Tenstorrent luwen
// SPDX-License-Identifier: Apache-2.0

//! Embedded Tenstorrent support extracted from luwen library
//! This module contains minimal functionality needed for tenstorrent device detection

pub mod arch;
pub mod chip;
pub mod detect;
pub mod error;
pub mod interface;
pub mod luwen_ref;
pub mod ttkmd;

pub use arch::Arch;
