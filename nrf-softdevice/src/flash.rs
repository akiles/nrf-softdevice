use core::future::Future;
use core::marker::PhantomData;
use core::pin::Pin;
use core::sync::atomic::{AtomicBool, Ordering};
use embassy::traits::flash::Error as FlashError;

use crate::fmt::{panic, *};
use crate::raw;
use crate::util::{DropBomb, Signal};
use crate::{RawError, Softdevice};

/// Singleton instance of the Flash softdevice functionality.
pub struct Flash {
    // Prevent Send, Sync
    _private: PhantomData<*mut ()>,
}

static FLASH_TAKEN: AtomicBool = AtomicBool::new(false);

impl Flash {
    const PAGE_SIZE: usize = 4096;

    /// Takes the Flash instance from the softdevice.
    ///
    /// # Panics
    ///
    /// Panics if called more than once.
    pub fn take(_sd: &Softdevice) -> Flash {
        if FLASH_TAKEN
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            panic!("nrf_softdevice::Softdevice::take_flash() called multiple times.")
        }

        Flash {
            _private: PhantomData,
        }
    }
}

static SIGNAL: Signal<Result<(), FlashError>> = Signal::new();

pub(crate) fn on_flash_success() {
    SIGNAL.signal(Ok(()))
}

pub(crate) fn on_flash_error() {
    SIGNAL.signal(Err(FlashError::Failed))
}

impl embassy::traits::flash::Flash for Flash {
    type ReadFuture<'a> = impl Future<Output = Result<(), FlashError>> + 'a;
    type WriteFuture<'a> = impl Future<Output = Result<(), FlashError>> + 'a;
    type ErasePageFuture<'a> = impl Future<Output = Result<(), FlashError>> + 'a;

    fn read<'a>(&'a mut self, address: usize, data: &'a mut [u8]) -> Self::ReadFuture<'a> {
        async move {
            // Reading is simple since SoC flash is memory-mapped :)
            // TODO check addr/len is in bounds.

            data.copy_from_slice(unsafe {
                core::slice::from_raw_parts(address as *const u8, data.len())
            });

            Ok(())
        }
    }

    fn write<'a>(&'a mut self, address: usize, data: &'a [u8]) -> Self::WriteFuture<'a> {
        async move {
            let data_ptr = data.as_ptr();
            let data_len = data.len() as u32;

            if address % 4 != 0 {
                return Err(FlashError::AddressMisaligned);
            }
            if (data_ptr as u32) % 4 != 0 || data_len % 4 != 0 {
                return Err(FlashError::BufferMisaligned);
            }

            // This is safe because we've checked ptr and len is aligned above
            let words_ptr = data_ptr as *const u32;
            let words_len = data_len / 4;

            let bomb = DropBomb::new();
            let ret = unsafe { raw::sd_flash_write(address as _, words_ptr, words_len) };
            let ret = match RawError::convert(ret) {
                Ok(()) => SIGNAL.wait().await,
                Err(_e) => {
                    warn!("sd_flash_write err {:?}", _e);
                    Err(FlashError::Failed)
                }
            };

            bomb.defuse();
            ret
        }
    }

    fn erase<'a>(&'a mut self, address: usize) -> Self::ErasePageFuture<'a> {
        async move {
            if address % Self::PAGE_SIZE != 0 {
                return Err(FlashError::AddressMisaligned);
            }

            let page_number = address / Self::PAGE_SIZE;

            let bomb = DropBomb::new();
            let ret = unsafe { raw::sd_flash_page_erase(page_number as u32) };
            let ret = match RawError::convert(ret) {
                Ok(()) => SIGNAL.wait().await,
                Err(_e) => {
                    warn!("sd_flash_page_erase err {:?}", _e);
                    Err(FlashError::Failed)
                }
            };

            bomb.defuse();
            ret
        }
    }

    fn size(&self) -> usize {
        256 * 4096
    }

    fn read_size(&self) -> usize {
        1
    }

    fn write_size(&self) -> usize {
        4
    }

    fn erase_size(&self) -> usize {
        4096
    }
}
