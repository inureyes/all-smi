// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Display;

use thiserror::Error;

use super::arch::Arch;

#[derive(Debug)]
pub struct BtWrapper(pub std::backtrace::Backtrace);

impl BtWrapper {
    #[inline(always)]
    pub fn capture() -> Self {
        Self(std::backtrace::Backtrace::capture())
    }
}

impl Display for BtWrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let std::backtrace::BacktraceStatus::Captured = self.0.status() {
            self.0.fmt(f)?;
        }
        Ok(())
    }
}

#[derive(Clone, Error, Debug)]
pub enum ArcReadyError {
    #[error("scratch register access failed")]
    NoAccess,
    #[error("ARC watchdog has triggered")]
    WatchdogTriggered,
    #[error("ARC FW has not yet booted")]
    BootIncomplete,
    #[error("ARC FW encountered an error during boot")]
    BootError,
    #[error("ARC is asleep")]
    Asleep,
    #[error("there is an outstanding PCIE DMA request")]
    OutstandingPcieDMA,
    #[error("another message is queued (0x{0:02x})")]
    MessageQueued(u32),
    #[error("another message is being procesed (0x{0:02x})")]
    HandlingMessage(u32),
    #[error("post code 0x{0:08x} indicates that you are running old fw... or that you aren't running any")]
    OldPostCode(u32),
}

#[derive(Error, Debug)]
pub enum PlatformError {
    #[error("Tried to initialize chip with the wrong architecture, expected {expected:?} but got {actual:?}\n{backtrace}")]
    WrongChipArch {
        actual: Arch,
        expected: Arch,
        backtrace: BtWrapper,
    },

    #[error("Tried to initialize chip with the wrong architecture, expected one of {expected:?} but got {actual:?}\n{backtrace}")]
    WrongChipArchs {
        actual: Arch,
        expected: Vec<Arch>,
        backtrace: BtWrapper,
    },

    #[error("Unsupported fw version, got {} but required {required:x}", version.map(|v| format!("{v:x}")).unwrap_or("<unknown version>".to_string()))]
    UnsupportedFwVersion { version: Option<u32>, required: u32 },

    #[error("It is not currently safe to communicate with ARC because, {0}\n{1}")]
    ArcNotReady(ArcReadyError, BtWrapper),

    #[error("Ethernet training not complete on {} ports", .0.iter().copied().filter(|v| *v).count())]
    EthernetTrainingNotComplete(Vec<bool>),

    #[error("{0}\n{1}")]
    Generic(String, BtWrapper),

    #[error("{0}\n{1}")]
    GenericError(Box<dyn std::error::Error>, BtWrapper),
}

impl From<Box<dyn std::error::Error>> for PlatformError {
    #[inline]
    fn from(e: Box<dyn std::error::Error>) -> Self {
        Self::GenericError(e, BtWrapper::capture())
    }
}

impl From<super::luwen_ref::LuwenError> for PlatformError {
    fn from(e: super::luwen_ref::LuwenError) -> Self {
        Self::Generic(e.to_string(), BtWrapper::capture())
    }
}

impl From<super::ttkmd::error::PciError> for PlatformError {
    fn from(e: super::ttkmd::error::PciError) -> Self {
        Self::GenericError(Box::new(e), BtWrapper::capture())
    }
}
