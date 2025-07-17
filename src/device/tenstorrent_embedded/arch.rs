// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0
// Extracted from luwen-core for embedded use

use std::fmt;
use std::str::FromStr;

#[derive(Clone, Hash, Copy, Debug, PartialEq, Eq)]
pub enum Arch {
    Grayskull,
    Wormhole,
    Blackhole,
}

impl Default for Arch {
    fn default() -> Self {
        Self::Grayskull
    }
}

impl Arch {
    pub fn is_wormhole(&self) -> bool {
        matches!(self, Arch::Wormhole)
    }

    pub fn is_grayskull(&self) -> bool {
        matches!(self, Arch::Grayskull)
    }

    pub fn is_blackhole(&self) -> bool {
        matches!(self, Arch::Blackhole)
    }
}

impl FromStr for Arch {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "grayskull" => Ok(Arch::Grayskull),
            "wormhole" => Ok(Arch::Wormhole),
            "blackhole" => Ok(Arch::Blackhole),
            err => Err(err.to_string()),
        }
    }
}

impl fmt::Display for Arch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Arch::Grayskull => write!(f, "Grayskull"),
            Arch::Wormhole => write!(f, "Wormhole"),
            Arch::Blackhole => write!(f, "Blackhole"),
        }
    }
}
