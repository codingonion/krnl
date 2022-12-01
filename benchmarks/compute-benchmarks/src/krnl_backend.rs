#[cfg(debug_assertions)]
use crate::saxpy_host;
#[cfg(debug_assertions)]
use approx::assert_relative_eq;
use krnl::{
    anyhow::Result,
    buffer::{Buffer, Slice},
    device::Device,
    future::BlockableFuture,
    kernel::module,
};

#[derive(Clone)]
pub struct KrnlBackend {
    device: Device,
}

impl KrnlBackend {
    pub fn new(index: usize) -> Result<Self> {
        Ok(Self {
            device: Device::new(index)?
        })
    }
    pub fn upload(&self, x: &[f32]) -> Result<()> {
        #[allow(unused)]
        let x_device = Slice::from(x).into_device(self.device.clone())?.block()?;
        self.device.sync()?.block()?;
        #[cfg(debug_assertions)] {
            let x_device = x_device.to_vec()?.block()?;
            assert_eq!(x, x_device.as_slice());
        }
        Ok(())
    }
    pub fn download(&self, x: &[f32]) -> Result<Download> {
        let x_device = Slice::from(x).into_device(self.device.clone())?.block()?;
        Ok(Download {
            x_device,
            #[cfg(debug_assertions)]
            x_host: x.to_vec(),
        })
    }
    pub fn saxpy(&self, x: &[f32], alpha: f32, y: &[f32]) -> Result<Saxpy> {
        assert_eq!(x.len(), y.len());
        let device = self.device.clone();
        let x_device = Slice::from(x)
            .into_device(device.clone())?
            .block()?;
        let y_device = Slice::from(y)
            .into_device(device.clone())?
            .block()?;
        #[cfg(debug_assertions)]
        let y_host = {
            let mut y_host = y.to_vec();
            saxpy_host(x, alpha, &mut y_host);
            y_host
        };
        Ok(Saxpy {
            device,
            x_device,
            alpha,
            y_device,
            #[cfg(debug_assertions)]
            y_host,
        })
    }
}

pub struct Download {
    x_device: Buffer<f32>,
    #[cfg(debug_assertions)]
    x_host: Vec<f32>,
}

impl Download {
    pub fn run(&self) -> Result<()> {
        #[allow(unused)]
        let x_device = self.x_device.to_vec()?.block()?;
        #[cfg(debug_assertions)] {
            assert_eq!(x_device, self.x_host);
        }
        Ok(())
    }
}

pub struct Saxpy {
    device: Device,
    x_device: Buffer<f32>,
    alpha: f32,
    y_device: Buffer<f32>,
    #[cfg(debug_assertions)]
    y_host: Vec<f32>,
}

impl Saxpy {
    pub fn run(&mut self) -> Result<()> {
        kernels::saxpy::build(self.device.clone())?
            .dispatch(self.x_device.as_slice(), self.alpha, self.y_device.as_slice_mut())?;
        self.device.sync()?.block()?;
        #[cfg(debug_assertions)] {
            let y_device = self.y_device.to_vec()?.block()?;
            assert_relative_eq!(self.y_host.as_slice(), y_device.as_slice());
        }
        Ok(())
    }
}

#[module]
mod kernels {
    use krnl_core::kernel;

    #[kernel(vulkan(1, 1), threads(256), elementwise)]
    pub fn saxpy(x: &f32, alpha: f32, y: &mut f32) {
        *y += alpha * *x;
    }
}
