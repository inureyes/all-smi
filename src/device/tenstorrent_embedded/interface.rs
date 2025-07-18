// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fs;

#[derive(Debug)]
pub enum FnNoc {
    Read {
        noc_id: u8,
        x: u32,
        y: u32,
        addr: u64,
        data: *mut u8,
        len: u64,
    },
    Write {
        noc_id: u8,
        x: u32,
        y: u32,
        addr: u64,
        data: *const u8,
        len: u64,
    },
    Multicast {
        noc_id: u8,
        start_x: u8,
        start_y: u8,
        end_x: u8,
        end_y: u8,
        addr: u64,
        data: *const u8,
        len: u64,
    },
    Broadcast {
        noc_id: u8,
        addr: u64,
        data: *const u8,
        len: u64,
    },
}

#[derive(Debug)]
pub enum FnAxi {
    Read {
        addr: u32,
        data: *mut u8,
        len: u32,
    },
    Write {
        addr: u32,
        data: *const u8,
        len: u32,
    },
}

#[derive(Clone, Debug)]
pub struct DeviceInfo {
    pub interface_id: u32,

    pub domain: u16,
    pub bus: u16,
    pub slot: u16,
    pub function: u16,

    pub vendor: u16,
    pub device_id: u16,
    pub board_id: u16,
    pub bar_size: Option<u64>,
}

impl DeviceInfo {
    /// Return the sysfs path for the PCIe device.
    fn pcie_base_path(&self) -> String {
        let domain = format!("{:04x}", self.domain);
        let bus = format!("{:02x}", self.bus);
        let slot = format!("{:02x}", self.slot);
        let function = format!("{:01x}", self.function);
        format!("/sys/bus/pci/devices/{domain}:{bus}:{slot}.{function}/")
    }

    /// Return link width; valid values of `s` are "current" and "max".
    fn pcie_link_width(&self, s: &str) -> u32 {
        let base_path = self.pcie_base_path();
        let path = format!("{}{}{}", &base_path, s, "_link_width");
        let width = fs::read_to_string(path)
            .map(|s| s.trim().to_string())
            .unwrap();
        width.parse::<u32>().unwrap()
    }

    /// Return link gen; valid values of `s` are "current" and "max".
    fn pcie_link_gen(&self, s: &str) -> i32 {
        let base_path = self.pcie_base_path();
        let path = format!("{}{}{}", &base_path, s, "_link_speed");
        let speed = fs::read_to_string(path)
            .map(|s| s.trim().to_string())
            .unwrap();
        match speed.split_whitespace().next().unwrap_or("") {
            "2.5" => 1,
            "5.0" => 2,
            "8.0" => 3,
            "16.0" => 4,
            "32.0" => 5,
            "64.0" => 6,
            _ => -1,
        }
    }

    /// Return the current PCIe link width.
    pub fn pcie_current_link_width(&self) -> u32 {
        self.pcie_link_width("current")
    }

    /// Return the current PCIe link generation.
    pub fn pcie_current_link_gen(&self) -> i32 {
        self.pcie_link_gen("current")
    }

    /// Return the maximum PCIe link width.
    pub fn pcie_max_link_width(&self) -> u32 {
        self.pcie_link_width("max")
    }

    /// Return the maximum PCIe link generation.
    pub fn pcie_max_link_gen(&self) -> i32 {
        self.pcie_link_gen("max")
    }
}

#[derive(Debug)]
pub enum FnDriver {
    DeviceInfo(*mut Option<DeviceInfo>),
}

#[derive(Debug)]
pub enum FnOptions {
    Driver(FnDriver),
    Axi(FnAxi),
    Noc(FnNoc),
}

type LuwenInterfaceCallback<T> = fn(&T, FnOptions) -> Result<(), Box<dyn std::error::Error>>;

#[derive(Clone)]
pub struct CallbackStorage<T: Clone + Send + Sync> {
    pub callback: LuwenInterfaceCallback<T>,
    pub user_data: T,
}

impl<T: Clone + Send + Sync> CallbackStorage<T> {
    pub fn new(callback: LuwenInterfaceCallback<T>, user_data: T) -> Self {
        Self {
            callback,
            user_data,
        }
    }
}

use super::ttkmd::kmdif::ChipInterface;

impl<T: Clone + Send + Sync + 'static> ChipInterface for CallbackStorage<T> {
    fn axi_read(&self, addr: u32, data: &mut [u8]) -> Result<(), Box<dyn std::error::Error>> {
        (self.callback)(
            &self.user_data,
            FnOptions::Axi(FnAxi::Read {
                addr,
                data: data.as_mut_ptr(),
                len: data.len() as u32,
            }),
        )
    }

    fn axi_write(&self, addr: u32, data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        (self.callback)(
            &self.user_data,
            FnOptions::Axi(FnAxi::Write {
                addr,
                data: data.as_ptr(),
                len: data.len() as u32,
            }),
        )
    }

    fn noc_read(
        &self,
        noc_id: u8,
        x: u8,
        y: u8,
        addr: u64,
        data: &mut [u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        (self.callback)(
            &self.user_data,
            FnOptions::Noc(FnNoc::Read {
                noc_id,
                x: x as u32,
                y: y as u32,
                addr,
                data: data.as_mut_ptr(),
                len: data.len() as u64,
            }),
        )
    }

    fn noc_write(
        &self,
        noc_id: u8,
        x: u8,
        y: u8,
        addr: u64,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        (self.callback)(
            &self.user_data,
            FnOptions::Noc(FnNoc::Write {
                noc_id,
                x: x as u32,
                y: y as u32,
                addr,
                data: data.as_ptr(),
                len: data.len() as u64,
            }),
        )
    }

    fn noc_broadcast(
        &self,
        noc_id: u8,
        addr: u64,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        (self.callback)(
            &self.user_data,
            FnOptions::Noc(FnNoc::Broadcast {
                noc_id,
                addr,
                data: data.as_ptr(),
                len: data.len() as u64,
            }),
        )
    }

    fn noc_multicast(
        &self,
        noc_id: u8,
        start: (u8, u8),
        end: (u8, u8),
        addr: u64,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        (self.callback)(
            &self.user_data,
            FnOptions::Noc(FnNoc::Multicast {
                noc_id,
                start_x: start.0,
                start_y: start.1,
                end_x: end.0,
                end_y: end.1,
                addr,
                data: data.as_ptr(),
                len: data.len() as u64,
            }),
        )
    }
}
