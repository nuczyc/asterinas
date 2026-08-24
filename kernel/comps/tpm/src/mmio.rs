use core::ops::Range;

use ostd::{
    io::IoMem,
    mm::{Paddr, VmIoOnce},
    task::Task,
};
use tpm_core::{TisMmio as CoreTisMmio, TisMmioBackend, phy::TisPhy, tis::TisErr};

pub struct TpmMmioBackend {
    /// ostd 发放的 MMIO 句柄。所有寄存器/数据口访问都以它为基址,偏移即
    /// `TPM_ACCESS(l)` 等地址里已折进 `l << 12` 的那个值。
    mmio: IoMem,
    polls_since_yield: u8,
}

impl TpmMmioBackend {
    /// 向 ostd 申领一段 MMIO 区并建立句柄。
    ///
    /// `phys` 是寄存器窗口的物理地址范围,长度须覆盖所有 locality 窗口（从 0 到
    /// `MAX_LOCALITY - 1` 共 5 个窗口，至少需要 `0x5000` 字节）。
    /// 映射、对齐、以及「这段区域确属 I/O 内存」由 ostd 的分配器核验;申领不到
    /// (地址不在允许的 MMIO 区、已被占用)返回 [`TisErr::Phy`]。
    fn acquire(phys: Range<Paddr>) -> Result<Self, TisErr> {
        match IoMem::acquire(phys) {
            Ok(mmio) => Ok(Self {
                mmio,
                polls_since_yield: 0,
            }),
            Err(_) => Err(TisErr::Phy),
        }
    }
}

impl TisMmioBackend for TpmMmioBackend {
    fn read8(&mut self, addr: u32) -> Result<u8, TisErr> {
        self.mmio
            .read_once::<u8>(addr as usize)
            .map_err(|_| TisErr::Phy)
    }

    fn read32(&mut self, addr: u32) -> Result<u32, TisErr> {
        self.mmio
            .read_once::<u32>(addr as usize)
            .map_err(|_| TisErr::Phy)
    }

    fn write8(&mut self, addr: u32, value: u8) -> Result<(), TisErr> {
        self.mmio
            .write_once::<u8>(addr as usize, &value)
            .map_err(|_| TisErr::Phy)
    }

    fn write32(&mut self, addr: u32, value: u32) -> Result<(), TisErr> {
        self.mmio
            .write_once::<u32>(addr as usize, &value.to_le())
            .map_err(|_| TisErr::Phy)
    }

    fn delay(&mut self) {
        const POLLS_BEFORE_YIELD: u8 = 64;

        self.polls_since_yield += 1;
        if self.polls_since_yield < POLLS_BEFORE_YIELD {
            core::hint::spin_loop();
            return;
        }

        self.polls_since_yield = 0;
        if Task::current().is_some() {
            Task::yield_now();
        } else {
            core::hint::spin_loop();
        }
    }
}

/// Preserve the existing comps-facing `TisMmio` name while adapting to the
/// generic backend-based `tpm-core::TisMmio`.
pub struct TisMmio {
    inner: CoreTisMmio<TpmMmioBackend>,
}

impl TisMmio {
    pub fn acquire(phys: Range<Paddr>) -> Result<Self, TisErr> {
        let backend = TpmMmioBackend::acquire(phys)?;
        Ok(Self {
            inner: CoreTisMmio::new(backend, 0),
        })
    }
}

impl TisPhy for TisMmio {
    fn read8(&mut self, addr: u32) -> Result<u8, TisErr> {
        self.inner.read8(addr)
    }

    fn read32(&mut self, addr: u32) -> Result<u32, TisErr> {
        self.inner.read32(addr)
    }

    fn write8(&mut self, addr: u32, value: u8) -> Result<(), TisErr> {
        self.inner.write8(addr, value)
    }

    fn write32(&mut self, addr: u32, value: u32) -> Result<(), TisErr> {
        self.inner.write32(addr, value)
    }

    fn read_fifo(&mut self, addr: u32, out: &mut [u8], off: usize, n: usize) -> Result<(), TisErr> {
        self.inner.read_fifo(addr, out, off, n)
    }

    fn write_fifo(&mut self, addr: u32, data: &[u8], off: usize, n: usize) -> Result<(), TisErr> {
        self.inner.write_fifo(addr, data, off, n)
    }

    fn reset_fifo(&mut self, addr: u32) {
        self.inner.reset_fifo(addr)
    }

    fn delay(&mut self) {
        self.inner.delay()
    }
}
