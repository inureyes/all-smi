# Tenstorrent Device Integration Note

## Overview

This note provides a comprehensive guide for integrating Tenstorrent NPU (Neural Processing Unit) devices into the all-smi monitoring system. Based on analysis of the `luwen`, `tt-tools-common`, and `tt-smi` reference implementations, this document covers device access, data reading, interpretation, and metric extraction.

## Architecture Overview

### Component Stack by Tenstorrent

```
┌─────────────────┐
│     tt-smi      │  (Terminal UI)
├─────────────────┤
│ tt-tools-common │  (Python utilities)
├─────────────────┤
│     pyluwen     │  (Python bindings)
├─────────────────┤
│      luwen      │  (Core Rust library)
├─────────────────┤
│  PCIe/Memory    │  (Hardware interface)
└─────────────────┘
```

### Key Technologies
- **PCIe BAR mapping**: Direct memory-mapped I/O access to device registers
- **ARC messaging**: Communication protocol with device firmware
- **AXI register access**: Memory-mapped register reads/writes
- **Telemetry struct**: Fixed memory layout containing all device metrics

## Device Access

### 1. Device Discovery

Tenstorrent devices are discovered via PCIe bus scanning:

```rust
// Detect devices with vendor ID 0x1e52
pub fn detect_tenstorrent_devices() -> Result<Vec<Device>, Error> {
    let mut devices = Vec::new();
    
    // Scan /dev/tenstorrent/ directory for character devices
    let entries = fs::read_dir("/dev/tenstorrent/")?;
    
    for entry in entries {
        let path = entry.path();
        if let Some(name) = path.file_name().to_str() {
            // Device files are named with numeric IDs (0, 1, 2, etc.)
            if let Ok(device_id) = name.parse::<usize>() {
                // Open the device file
                let device = open_device(device_id)?;
                devices.push(device);
            }
        }
    }
    
    Ok(devices)
}
```

### 2. Device Initialization

```rust
pub struct TenstorrentDevice {
    device_id: usize,
    file: File,
    arch: Arch,
    bar_mappings: HashMap<u32, BarMapping>,
    telemetry_addr: Option<u32>,
}

impl TenstorrentDevice {
    pub fn open(device_id: usize) -> Result<Self, Error> {
        // Open character device
        let path = format!("/dev/tenstorrent/{}", device_id);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)?;
        
        // Get device info via ioctl
        let device_info = unsafe {
            let mut info = MaybeUninit::<DeviceInfo>::uninit();
            ioctl_get_device_info(file.as_raw_fd(), info.as_mut_ptr())?;
            info.assume_init()
        };
        
        // Determine architecture from device ID
        let arch = match device_info.device_id {
            0xfaca => Arch::Grayskull,
            0x401e => Arch::Wormhole,  
            0xb140 => Arch::Blackhole,
            _ => return Err(Error::UnknownDevice),
        };
        
        Ok(Self {
            device_id,
            file,
            arch,
            bar_mappings: HashMap::new(),
            telemetry_addr: None,
        })
    }
}
```

### 3. BAR Mapping

PCIe Base Address Register (BAR) mapping is essential for memory-mapped I/O:

```rust
#[repr(C)]
struct QueryMappings {
    mappings: [PciBarMapping; 6],
    mapping_count: u32,
}

pub fn map_bar(&mut self) -> Result<(), Error> {
    // Query available mappings
    let mut query = QueryMappings::default();
    unsafe {
        ioctl_query_mappings(self.file.as_raw_fd(), &mut query)?;
    }
    
    // Map each BAR
    for i in 0..query.mapping_count as usize {
        let mapping = &query.mappings[i];
        if mapping.mapping_id == 0 {
            continue; // Unused mapping
        }
        
        // Allocate TLB for BAR mapping
        let mut alloc = AllocateDmaBuffer {
            requested_size: mapping.mapping_size as usize,
            physical_address: 0,
            mapping_id: mapping.mapping_id,
        };
        
        unsafe {
            ioctl_allocate_tlb(self.file.as_raw_fd(), &mut alloc)?;
        }
        
        // Memory map the BAR
        let mmap_offset = mapping.mapping_id as i64 * (1 << 28); // 256MB per mapping
        let ptr = unsafe {
            mmap(
                std::ptr::null_mut(),
                mapping.mapping_size as usize,
                PROT_READ | PROT_WRITE,
                MAP_SHARED,
                self.file.as_raw_fd(),
                mmap_offset,
            )
        };
        
        self.bar_mappings.insert(mapping.mapping_id, BarMapping {
            base_addr: ptr as *mut u8,
            size: mapping.mapping_size,
        });
    }
    
    Ok(())
}
```

## Reading Device Data

### 1. ARC Message Protocol

Communication with device firmware uses the ARC (Argonaut RISC Core) message protocol:

```rust
#[repr(C)]
pub struct ArcMsg {
    msg_type: u16,
    msg_code: u8,
    return_code: u8,
    arg0: u32,
    arg1: u32,
}

pub enum TypedArcMsg {
    Nop = 0x11,
    GetSmbusTelemetryAddr = 0x2C,
    // ... other message types
}

pub fn send_arc_message(&self, msg: TypedArcMsg) -> Result<u32, Error> {
    // Prepare message
    let arc_msg = ArcMsg {
        msg_type: 0xaa55,  // Magic header
        msg_code: msg as u8,
        return_code: 0,
        arg0: 0,
        arg1: 0,
    };
    
    // Write message to scratch registers
    let scratch_base = self.axi_translate("ARC_RESET.SCRATCH[0]")?;
    self.axi_write32(scratch_base, u32::from_le_bytes(arc_msg.to_bytes()[0..4]))?;
    self.axi_write32(scratch_base + 4, u32::from_le_bytes(arc_msg.to_bytes()[4..8]))?;
    self.axi_write32(scratch_base + 8, u32::from_le_bytes(arc_msg.to_bytes()[8..12]))?;
    
    // Trigger doorbell
    let misc_cntl = self.axi_translate("ARC_RESET.ARC_MISC_CNTL")?;
    self.axi_write32(misc_cntl, 1 << 5)?; // Set doorbell bit
    
    // Wait for response
    let start = Instant::now();
    loop {
        let msg_type = self.axi_read32(scratch_base)?;
        
        if (msg_type & 0xffff) == 0x55aa {
            // Response received
            let return_code = self.axi_read32(scratch_base + 4)? & 0xff;
            if return_code == 0 {
                let result = self.axi_read32(scratch_base + 8)?;
                return Ok(result);
            }
        }
        
        if start.elapsed() > Duration::from_secs(1) {
            return Err(Error::Timeout);
        }
    }
}
```

### 2. Register Name Translation

The system uses symbolic register names that must be translated to addresses:

```rust
pub fn axi_translate(reg_name: &str) -> Result<u64, Error> {
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
        
        _ => Err(Error::UnknownRegister),
    }
}
```

### 3. Telemetry Reading

```rust
pub fn get_telemetry(&self) -> Result<Telemetry, Error> {
    // First, get telemetry address if not cached
    if self.telemetry_addr.is_none() {
        let addr = self.send_arc_message(TypedArcMsg::GetSmbusTelemetryAddr)?;
        self.telemetry_addr = Some(addr);
    }
    
    // Calculate actual offset in CSM
    let telemetry_addr = self.telemetry_addr.unwrap();
    let csm_base = self.axi_translate("ARC_CSM.DATA[0]")?;
    let offset = csm_base + (telemetry_addr - 0x10000000) as u64;
    
    // Read telemetry struct fields (each field is 4 bytes)
    let mut telemetry = Telemetry::default();
    telemetry.enum_version = self.axi_read32(offset + 0)?;
    telemetry.device_id = self.axi_read32(offset + 4)?;
    telemetry.asic_ro = self.axi_read32(offset + 8)?;
    telemetry.asic_idd = self.axi_read32(offset + 12)?;
    telemetry.board_id_high = self.axi_read32(offset + 16)?;
    telemetry.board_id_low = self.axi_read32(offset + 20)?;
    // ... continue reading all fields at 4-byte offsets
    
    telemetry.vcore = self.axi_read32(offset + 112)?;        // offset 28*4
    telemetry.asic_temperature = self.axi_read32(offset + 116)?; // offset 29*4
    telemetry.tdp = self.axi_read32(offset + 128)?;          // offset 32*4
    telemetry.tdc = self.axi_read32(offset + 132)?;          // offset 33*4
    telemetry.aiclk = self.axi_read32(offset + 96)?;         // offset 24*4
    
    Ok(telemetry)
}
```

## Data Interpretation

### 1. Temperature Calculation

Temperature encoding varies by architecture:

```rust
pub fn calculate_temperature(arch: Arch, raw_value: u32) -> f64 {
    match arch {
        Arch::Blackhole => {
            // Blackhole uses signed 16.16 fixed-point format
            let value = raw_value as i32;
            let int_part = (value >> 16) as i16 as f64;
            let frac_part = (value & 0xFFFF) as f64 / 65536.0;
            int_part + frac_part
        }
        Arch::Wormhole | Arch::Grayskull => {
            // Temperature is in lower 16 bits, divided by 16
            ((raw_value & 0xFFFF) as f64) / 16.0
        }
    }
}
```

### 2. Power and Current Extraction

Power and current values use the lower 16 bits:

```rust
pub fn extract_power(tdp_register: u32) -> f64 {
    // Current power is in lower 16 bits
    (tdp_register & 0xFFFF) as f64
}

pub fn extract_current(tdc_register: u32) -> f64 {
    // Current draw is in lower 16 bits
    (tdc_register & 0xFFFF) as f64
}

pub fn extract_tdp_limit(tdp_register: u32) -> f64 {
    // TDP limit is in upper 16 bits
    ((tdp_register >> 16) & 0xFFFF) as f64
}

pub fn extract_tdc_limit(tdc_register: u32) -> f64 {
    // TDC limit is in upper 16 bits
    ((tdc_register >> 16) & 0xFFFF) as f64
}
```

### 3. Clock Frequencies

```rust
pub fn extract_ai_clock(aiclk_register: u32) -> u32 {
    // AI clock frequency in MHz is in lower 16 bits
    aiclk_register & 0xFFFF
}

pub fn extract_ai_clock_max(aiclk_register: u32) -> u32 {
    // Maximum AI clock is in upper 16 bits
    (aiclk_register >> 16) & 0xFFFF
}
```

### 4. Board Identification

```rust
pub fn get_board_type(board_serial: u64) -> &'static str {
    match (board_serial >> 36) & 0xFFFFF {
        0x1 => match (board_serial >> 32) & 0xF {
            0x2 => "E300_R2",
            0x3 | 0x4 => "E300_R3",
            _ => "Unknown",
        },
        0x3 => "e150",
        0x7 => "e75",
        0x8 => "NEBULA_CB",
        0xA => "e300",
        0xB => "GALAXY",
        0x14 => "n300",
        0x18 => "n150",
        0x35 => "galaxy-wormhole",
        0x36 => "p100",
        0x40 => "p150a",
        0x41 => "p150b",
        0x42 => "p150c",
        0x43 => "p100a",
        0x44 => "p300b",
        0x45 => "p300a",
        0x46 => "p300c",
        0x47 => "galaxy-blackhole",
        _ => "Unknown",
    }
}
```

## Metrics for all-smi

Based on the reference implementations, the following metrics are extracted:

### Core Metrics
1. **Device Identification**
   - Board ID (64-bit serial number as hex)
   - Board Type (decoded from serial number)
   - Architecture (Grayskull/Wormhole/Blackhole)
   - Device Name (combination of arch + board type)

2. **Performance Metrics**
   - AI Clock Frequency (MHz) - `telemetry.aiclk & 0xFFFF`
   - AXI Clock Frequency (MHz) - `telemetry.axiclk`
   - ARC Clock Frequency (MHz) - `telemetry.arcclk`

3. **Power Metrics**
   - Power Consumption (W) - `telemetry.tdp & 0xFFFF`
   - Current Draw (A) - `telemetry.tdc & 0xFFFF`
   - Voltage (V) - `telemetry.vcore / 1000.0`

4. **Thermal Metrics**
   - ASIC Temperature (°C) - Architecture-specific calculation
   - Voltage Regulator Temperature (°C) - `telemetry.vreg_temperature & 0xFFFF`
   - Board Temperatures (inlet/outlet) - Extracted from `board_temperature`

5. **Memory Information**
   - Total Memory - Board-specific lookup (16GB-576GB depending on model)
   - Memory Usage - Estimated based on power consumption
   - DDR Status - `telemetry.ddr_status`

6. **Firmware Versions**
   - ARC Firmware - Decoded as MAJOR.MINOR.PATCH
   - Ethernet Firmware - Decoded as MAJOR.MINOR.PATCH
   - Firmware Date - Decoded from `wh_fw_date`

7. **Health Status**
   - Heartbeat Counter - `arc0_health` for GS, `arc3_health` for WH
   - PCIe Status - `telemetry.pcie_status`
   - Ethernet Status - `telemetry.eth_status0/1`

### Utilization Calculation

Since Tenstorrent doesn't provide direct utilization metrics, estimate based on power:

```rust
pub fn calculate_utilization(telemetry: &Telemetry) -> f64 {
    let current_power = extract_power(telemetry.tdp);
    let tdp_limit = get_board_tdp(telemetry.board_type());
    
    ((current_power / tdp_limit) * 100.0).min(100.0)
}

fn get_board_tdp(board_type: &str) -> f64 {
    match board_type {
        // Grayskull boards
        "e75" => 75.0,
        "e150" => 75.0,
        "e300" | "E300_R2" | "E300_R3" => 100.0,
        "GALAXY" => 300.0,
        
        // Wormhole boards
        "n150" => 150.0,
        "n300" => 160.0,
        "NEBULA_CB" => 150.0,
        "galaxy-wormhole" => 200.0,
        
        // Blackhole boards
        "p100" | "p100a" => 300.0,
        "p150a" | "p150b" | "p150c" => 350.0,
        "p300a" | "p300b" | "p300c" => 400.0,
        "galaxy-blackhole" => 450.0,
        
        _ => 200.0, // Conservative default
    }
}
```

## Brief Development Notes for `all-smi`

1. **Error Handling**: The value 0x66666666 is NOT an error - it's valid telemetry data that needs bit masking

2. **Initialization**: Always wait for chip initialization before reading telemetry:
   ```rust
   // Wait for ARC firmware to be ready
   wait_for_arc_ready()?;
   
   // Verify heartbeat is incrementing
   verify_heartbeat_active()?;
   ```

3. **Caching**: Cache the telemetry address after first retrieval to avoid repeated ARC messages

4. **Memory Safety**: Use proper memory barriers when reading from memory-mapped regions

5. **Multi-chip Support**: Handle both local and remote chips (Wormhole supports ethernet-connected remote chips)
