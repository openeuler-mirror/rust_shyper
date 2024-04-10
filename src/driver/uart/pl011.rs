// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use tock_registers::interfaces::*;
use tock_registers::register_structs;
use tock_registers::registers::*;

/// Flag indicating that the receive FIFO is full.
const UART_FR_RXFF: u32 = 1 << 4;
/// Flag indicating that the transmit FIFO is full.
const UART_FR_TXFF: u32 = 1 << 5;

/// Register struct representing the PL011 MMIO block.
register_structs! {
  #[allow(non_snake_case)]
  pub Pl011Mmio {
    (0x000 => pub Data: ReadWrite<u32>),
    (0x004 => pub RecvStatusErrClr: ReadWrite<u32>),
    (0x008 => _reserved_1),
    (0x018 => pub Flag: ReadOnly<u32>),
    (0x01c => _reserved_2),
    (0x020 => pub IrDALowPower: ReadWrite<u32>),
    (0x024 => pub IntBaudRate: ReadWrite<u32>),
    (0x028 => pub FracBaudRate: ReadWrite<u32>),
    (0x02c => pub LineControl: ReadWrite<u32>),
    (0x030 => pub Control: ReadWrite<u32>),
    (0x034 => pub IntFIFOLevel: ReadWrite<u32>),
    (0x038 => pub IntMaskSetClr: ReadWrite<u32>),
    (0x03c => pub RawIntStatus: ReadOnly<u32>),
    (0x040 => pub MaskedIntStatus: ReadOnly<u32>),
    (0x044 => pub IntClear: WriteOnly<u32>),
    (0x048 => pub DmaControl: ReadWrite<u32>),
    (0x04c => _reserved_3),
    (0x1000 => @END),
  }
}

impl super::UartOperation for Pl011Mmio {
    #[inline]
    fn init(&self) {}

    /// Sends a byte through the UART device.
    #[inline]
    fn send(&self, byte: u8) {
        while self.Flag.get() & UART_FR_TXFF != 0 {
            core::hint::spin_loop();
        }
        self.Data.set(byte as u32);
    }
}
