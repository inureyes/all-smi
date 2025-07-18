// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

#[derive(Default, Debug, Clone, Copy)]
#[repr(C)]
pub struct Tlb {
    pub local_offset: u64,
    pub x_end: u8,
    pub y_end: u8,
    pub x_start: u8,
    pub y_start: u8,
    pub noc_sel: u8,
    pub mcast: bool,
    pub ordering: TlbOrdering,
    pub linked: bool,
    pub static_vc: u8,
}

impl Tlb {
    pub fn new(x: u8, y: u8, addr: u64) -> Self {
        Self {
            x_end: x,
            y_end: y,
            local_offset: addr,
            ..Default::default()
        }
    }

    pub fn multicast(x_start: u8, y_start: u8, x_end: u8, y_end: u8, addr: u64) -> Self {
        Self {
            x_start,
            y_start,
            x_end,
            y_end,
            local_offset: addr,
            mcast: true,
            ..Default::default()
        }
    }

    pub fn broadcast(x_start: u8, y_start: u8, x_end: u8, y_end: u8, addr: u64) -> Self {
        Self {
            x_start,
            y_start,
            x_end,
            y_end,
            local_offset: addr,
            mcast: true,
            ..Default::default()
        }
    }
}

#[derive(Default, Debug, Clone, Copy)]
pub enum TlbOrdering {
    #[default]
    Posted,
    Relaxed,
    Strict,
}

impl From<TlbOrdering> for u8 {
    fn from(val: TlbOrdering) -> Self {
        match val {
            TlbOrdering::Posted => 0,
            TlbOrdering::Relaxed => 1,
            TlbOrdering::Strict => 2,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DeviceTlbInfo {
    pub name: &'static str,
    pub address: u32,
    pub size: u32,
}

/// DMA buffer allocation structure for TLB mapping
#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
pub struct AllocateDmaBuffer {
    pub requested_size: usize,
    pub physical_address: u64,
    pub mapping_id: u32,
}
