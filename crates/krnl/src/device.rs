use crate::{
    buffer::{ScalarSlice, ScalarSliceMut, Slice, SliceMut},
    scalar::{Scalar, ScalarElem, ScalarType},
};
use anyhow::{bail, Result};
#[cfg(feature = "device")]
use rspirv::{binary::Assemble, dr::Operand};
use serde::Deserialize;
#[cfg(feature = "device")]
use std::{collections::HashMap, hash::Hash, ops::Range};
use std::{
    fmt::{self, Debug},
    sync::Arc,
};

#[cfg(feature = "device")]
mod vulkan_engine;
#[cfg(feature = "device")]
use vulkan_engine::Engine;

mod error {
    use std::fmt::{self, Debug, Display};

    #[derive(Clone, Copy, Debug, thiserror::Error)]
    #[error("DeviceUnavailable")]
    pub(super) struct DeviceUnavailable;

    #[cfg(feature = "device")]
    #[derive(Clone, Copy, Debug, thiserror::Error)]
    pub(super) struct DeviceIndexOutOfRange {
        #[allow(unused)]
        pub(super) index: usize,
        #[allow(unused)]
        pub(super) devices: usize,
    }

    #[cfg(feature = "device")]
    impl Display for DeviceIndexOutOfRange {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            Debug::fmt(self, f)
        }
    }

    #[derive(Clone, Copy, thiserror::Error)]
    pub struct DeviceLost {
        #[cfg(feature = "device")]
        pub(super) index: usize,
        #[cfg(feature = "device")]
        pub(super) handle: u64,
    }

    impl Debug for DeviceLost {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            #[cfg(feature = "device")]
            {
                f.debug_tuple("DeviceLost")
                    .field(&self.index)
                    .field(&(self.handle as *const ()))
                    .finish()
            }
            #[cfg(not(feature = "device"))]
            {
                write!(f, "DeviceLost")
            }
        }
    }

    impl Display for DeviceLost {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            Debug::fmt(self, f)
        }
    }
}
use error::*;

pub mod builder {
    use super::*;

    pub struct DeviceBuilder {
        #[cfg(feature = "device")]
        pub(super) options: DeviceOptions,
    }

    impl DeviceBuilder {
        pub fn index(self, index: usize) -> Self {
            #[cfg(feature = "device")]
            {
                let mut this = self;
                this.options.index = index;
                this
            }
            #[cfg(not(feature = "device"))]
            {
                let _ = index;
                self
            }
        }
        pub fn build(self) -> Result<Device> {
            #[cfg(feature = "device")]
            {
                let raw = RawDevice::new(self.options)?;
                Ok(Device {
                    inner: DeviceInner::Device(raw),
                })
            }
            #[cfg(not(feature = "device"))]
            {
                Err(DeviceUnavailable.into())
            }
        }
    }
}
use builder::*;

#[cfg(feature = "device")]
trait DeviceEngine {
    type DeviceBuffer: DeviceEngineBuffer<Engine = Self>;
    type Kernel: DeviceEngineKernel<Engine = Self, DeviceBuffer = Self::DeviceBuffer>;
    fn new(options: DeviceOptions) -> Result<Arc<Self>>;
    fn handle(&self) -> u64;
    fn info(&self) -> &Arc<DeviceInfo>;
    fn wait(&self) -> Result<(), DeviceLost>;
    //fn performance_metrics(&self) -> PerformanceMetrics;
}

#[cfg(feature = "device")]
struct DeviceOptions {
    index: usize,
    optimal_features: Features,
}

#[cfg(feature = "device")]
trait DeviceEngineBuffer: Sized {
    type Engine;
    unsafe fn uninit(engine: Arc<Self::Engine>, len: usize) -> Result<Self>;
    fn upload(&self, data: &[u8]) -> Result<()>;
    fn download(&self, data: &mut [u8]) -> Result<()>;
    fn transfer(&self, dst: &Self) -> Result<()>;
    fn engine(&self) -> &Arc<Self::Engine>;
    fn offset(&self) -> usize;
    fn len(&self) -> usize;
    fn slice(self: &Arc<Self>, range: Range<usize>) -> Option<Arc<Self>>;
}

#[cfg(feature = "device")]
trait DeviceEngineKernel: Sized {
    type Engine;
    type DeviceBuffer;
    fn cached(
        engine: Arc<Self::Engine>,
        key: KernelKey,
        desc_fn: impl FnOnce() -> Result<Arc<KernelDesc>>,
    ) -> Result<Arc<Self>>;
    unsafe fn dispatch(
        &self,
        groups: [u32; 3],
        buffers: &[Arc<Self::DeviceBuffer>],
        push_consts: Vec<u8>,
    ) -> Result<()>;
    fn engine(&self) -> &Arc<Self::Engine>;
    fn desc(&self) -> &Arc<KernelDesc>;
}

#[derive(Clone, Eq, PartialEq)]
pub struct Device {
    inner: DeviceInner,
}

impl Device {
    pub const fn host() -> Self {
        Self {
            inner: DeviceInner::Host,
        }
    }
    pub fn builder() -> DeviceBuilder {
        DeviceBuilder {
            #[cfg(feature = "device")]
            options: DeviceOptions {
                index: 0,
                optimal_features: Features::empty()
                    .with_shader_int8(true)
                    .with_shader_int16(true)
                    .with_shader_int64(true)
                    .with_shader_float16(true)
                    .with_shader_float64(true),
            },
        }
    }
    pub fn is_host(&self) -> bool {
        self.inner.is_host()
    }
    pub fn is_device(&self) -> bool {
        self.inner.is_device()
    }
    pub(crate) fn inner(&self) -> &DeviceInner {
        &self.inner
    }
    pub fn info(&self) -> Option<&Arc<DeviceInfo>> {
        match self.inner() {
            DeviceInner::Host => None,
            #[cfg(feature = "device")]
            DeviceInner::Device(raw) => Some(raw.info()),
        }
    }
    pub fn wait(&self) -> Result<(), DeviceLost> {
        match self.inner() {
            DeviceInner::Host => Ok(()),
            #[cfg(feature = "device")]
            DeviceInner::Device(raw) => raw.wait(),
        }
    }
}

impl Debug for Device {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

#[cfg(feature = "device")]
impl From<RawDevice> for Device {
    fn from(device: RawDevice) -> Self {
        Self {
            inner: DeviceInner::Device(device),
        }
    }
}

#[derive(Clone, Eq, PartialEq, derive_more::Unwrap)]
pub(crate) enum DeviceInner {
    Host,
    #[cfg(feature = "device")]
    Device(RawDevice),
}

impl DeviceInner {
    pub(crate) fn is_host(&self) -> bool {
        #[cfg_attr(not(feature = "device"), allow(irrefutable_let_patterns))]
        if let Self::Host = self {
            true
        } else {
            false
        }
    }
    pub(crate) fn is_device(&self) -> bool {
        !self.is_host()
    }
}

impl Debug for DeviceInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Host => f.debug_struct("Host").finish(),
            #[cfg(feature = "device")]
            Self::Device(raw_device) => raw_device.fmt(f),
        }
    }
}

#[cfg(feature = "device")]
#[derive(Clone)]
pub(crate) struct RawDevice {
    engine: Arc<Engine>,
}

#[cfg(feature = "device")]
impl RawDevice {
    fn new(options: DeviceOptions) -> Result<Self> {
        let engine = Engine::new(options)?;
        Ok(Self { engine })
    }
    fn info(&self) -> &Arc<DeviceInfo> {
        self.engine.info()
    }
    fn wait(&self) -> Result<(), DeviceLost> {
        self.engine.wait()
    }
}

#[cfg(feature = "device")]
impl PartialEq for RawDevice {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.engine, &other.engine)
    }
}

#[cfg(feature = "device")]
impl Eq for RawDevice {}

#[cfg(feature = "device")]
impl Debug for RawDevice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let index = self.info().index;
        let handle = self.engine.handle() as *const ();
        f.debug_tuple("Device")
            .field(&index)
            .field(&handle)
            .finish()
    }
}

#[cfg(feature = "device")]
#[repr(transparent)]
#[derive(Clone)]
pub(crate) struct DeviceBuffer {
    inner: Arc<<Engine as DeviceEngine>::DeviceBuffer>,
}

#[cfg(feature = "device")]
impl DeviceBuffer {
    pub(crate) unsafe fn uninit(device: RawDevice, len: usize) -> Result<Self> {
        let inner =
            unsafe { <Engine as DeviceEngine>::DeviceBuffer::uninit(device.engine, len)?.into() };
        Ok(Self { inner })
    }
    pub(crate) fn upload(&self, data: &[u8]) -> Result<()> {
        self.inner.upload(data)
    }
    pub(crate) fn download(&self, data: &mut [u8]) -> Result<()> {
        self.inner.download(data)
    }
    pub(crate) fn transfer(&self, dst: &Self) -> Result<()> {
        self.inner.transfer(&dst.inner)
    }
    pub(crate) fn offset(&self) -> usize {
        self.inner.offset()
    }
    pub(crate) fn len(&self) -> usize {
        self.inner.len()
    }
    pub(crate) fn device(&self) -> RawDevice {
        RawDevice {
            engine: self.inner.engine().clone(),
        }
    }
    pub(crate) fn slice(&self, range: Range<usize>) -> Option<Self> {
        let inner = self.inner.slice(range)?;
        Some(Self { inner })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub struct Features {
    shader_int8: bool,
    shader_int16: bool,
    shader_int64: bool,
    shader_float16: bool,
    shader_float64: bool,
}

impl Features {
    pub const fn empty() -> Self {
        Self {
            shader_int8: false,
            shader_int16: false,
            shader_int64: false,
            shader_float16: false,
            shader_float64: false,
        }
    }
    pub const fn shader_int8(&self) -> bool {
        self.shader_int8
    }
    pub const fn with_shader_int8(mut self, shader_int8: bool) -> Self {
        self.shader_int8 = shader_int8;
        self
    }
    pub const fn shader_int16(&self) -> bool {
        self.shader_int16
    }
    pub const fn with_shader_int16(mut self, shader_int16: bool) -> Self {
        self.shader_int16 = shader_int16;
        self
    }
    pub const fn shader_int64(&self) -> bool {
        self.shader_int64
    }
    pub const fn with_shader_int64(mut self, shader_int64: bool) -> Self {
        self.shader_int64 = shader_int64;
        self
    }
    pub const fn shader_float16(&self) -> bool {
        self.shader_float16
    }
    pub const fn with_shader_float16(mut self, shader_float16: bool) -> Self {
        self.shader_float16 = shader_float16;
        self
    }
    pub const fn shader_float64(&self) -> bool {
        self.shader_float64
    }
    pub const fn with_shader_float64(mut self, shader_float64: bool) -> Self {
        self.shader_float64 = shader_float64;
        self
    }
    pub const fn contains(&self, other: &Features) -> bool {
        (self.shader_int8 || !other.shader_int8)
            && (self.shader_int16 || !other.shader_int16)
            && (self.shader_int64 || !other.shader_int64)
            && (self.shader_float16 || !other.shader_float16)
            && (self.shader_float64 || !other.shader_float64)
    }
    pub const fn union(mut self, other: &Features) -> Self {
        self.shader_int8 |= other.shader_int8;
        self.shader_int16 |= other.shader_int16;
        self.shader_int64 |= other.shader_int64;
        self.shader_float16 |= other.shader_float16;
        self.shader_float64 |= other.shader_float64;
        self
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct DeviceInfo {
    index: usize,
    name: String,
    compute_queues: usize,
    transfer_queues: usize,
    features: Features,
}

impl DeviceInfo {
    pub fn features(&self) -> Features {
        self.features
    }
}

/*
#[derive(Clone, Copy, Debug)]
struct TransferMetrics {
    bytes: usize,
    time: Duration,
}

#[derive(Clone, Copy, Debug)]
struct KernelMetrics {
    dispatches: usize,
    time: Duration,
}

#[derive(Clone, Debug)]
pub struct PerformanceMetrics {
    upload: TransferMetrics,
    download: TransferMetrics,
    kernels: HashMap<String, KernelMetrics>,
}*/

/*
#[derive(Default, Clone)]
struct KernelKey {
    inner: Arc<()>,
    spec_consts: Vec<ScalarElem>,
}

impl PartialEq for KernelKey {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner) && self.spec_consts == other.spec_consts
    }
}

impl Eq for KernelKey {}

impl Hash for KernelKey {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        (Arc::as_ptr(&self.inner) as usize).hash(hasher);
        for spec in self.spec_consts.iter().copied() {
            use ScalarElem::*;
            match spec {
                U8(x) => x.hash(hasher),
                I8(x) => x.hash(hasher),
                U16(x) => x.hash(hasher),
                I16(x) => x.hash(hasher),
                F16(x) => x.to_bits().hash(hasher),
                BF16(x) => x.to_bits().hash(hasher),
                U32(x) => x.hash(hasher),
                I32(x) => x.hash(hasher),
                F32(x) => x.to_bits().hash(hasher),
                F64(x) => x.to_bits().hash(hasher),
                _ => unreachable!(),
            }
        }
    }
}*/

#[cfg_attr(not(feature = "device"), allow(dead_code))]
#[derive(Clone, Deserialize, Debug)]
struct KernelDesc {
    name: String,
    hash: u64,
    spirv: Vec<u32>,
    features: Features,
    threads: Vec<u32>,
    safe: bool,
    spec_descs: Vec<SpecDesc>,
    slice_descs: Vec<SliceDesc>,
    push_descs: Vec<PushDesc>,
}

#[cfg(feature = "device")]
impl KernelDesc {
    fn push_consts_range(&self) -> u32 {
        let mut size: usize = self.push_descs.iter().map(|x| x.scalar_type.size()).sum();
        while size % 4 != 0 {
            size += 1;
        }
        size += self.slice_descs.len() * 2 * 4;
        size.try_into().unwrap()
    }
    fn specialize(&self, threads: Vec<u32>, spec_consts: &[ScalarElem]) -> Result<Self> {
        use rspirv::spirv::{Decoration, Op};
        let mut module = rspirv::dr::load_words(&self.spirv).unwrap();
        let mut spec_ids = HashMap::<u32, u32>::with_capacity(spec_consts.len());
        for inst in module.annotations.iter() {
            if inst.class.opcode == Op::Decorate {
                if let [Operand::IdRef(id), Operand::Decoration(Decoration::SpecId), Operand::LiteralInt32(spec_id)] =
                    inst.operands.as_slice()
                {
                    spec_ids.insert(*id, *spec_id);
                }
            }
        }
        for inst in module.types_global_values.iter_mut() {
            if inst.class.opcode == Op::SpecConstant {
                if let Some(result_id) = inst.result_id {
                    if let Some(spec_id) = spec_ids.get(&result_id) {
                        if let Some(value) = spec_consts.get(*spec_id as usize) {
                            match inst.operands.as_mut_slice() {
                                [Operand::LiteralInt32(a)] => {
                                    bytemuck::bytes_of_mut(a).copy_from_slice(value.as_bytes());
                                }
                                [Operand::LiteralInt32(a), Operand::LiteralInt32(b)] => {
                                    bytemuck::bytes_of_mut(a)
                                        .copy_from_slice(&value.as_bytes()[..8]);
                                    bytemuck::bytes_of_mut(b)
                                        .copy_from_slice(&value.as_bytes()[9..]);
                                }
                                _ => unreachable!("{:?}", inst.operands),
                            }
                        }
                    }
                }
            }
        }
        let spirv = module.assemble();
        Ok(Self {
            spirv,
            spec_descs: Vec::new(),
            threads,
            ..self.clone()
        })
    }
}

#[cfg_attr(not(feature = "device"), allow(dead_code))]
#[derive(Clone, Deserialize, Debug)]
struct SpecDesc {
    #[allow(unused)]
    name: String,
    scalar_type: ScalarType,
    thread_dim: Option<usize>,
}

#[cfg_attr(not(feature = "device"), allow(dead_code))]
#[derive(Clone, Deserialize, Debug)]
struct SliceDesc {
    name: String,
    scalar_type: ScalarType,
    mutable: bool,
    item: bool,
}

#[cfg_attr(not(feature = "device"), allow(dead_code))]
#[derive(Clone, Deserialize, Debug)]
struct PushDesc {
    #[allow(unused)]
    name: String,
    scalar_type: ScalarType,
}

#[doc(hidden)]
#[cfg_attr(not(feature = "device"), allow(dead_code))]
#[derive(Clone)]
pub struct KernelBuilder {
    id: usize,
    desc: Arc<KernelDesc>,
    spec_consts: Vec<ScalarElem>,
    threads: [u32; 3],
}

impl KernelBuilder {
    pub fn from_bytes(bytes: &'static [u8]) -> Result<Self> {
        let desc: Arc<KernelDesc> = Arc::new(bincode2::deserialize(bytes)?);
        let mut threads = [1, 1, 1];
        threads[..desc.threads.len()].copy_from_slice(&desc.threads);
        Ok(Self {
            id: bytes.as_ptr() as usize,
            desc,
            spec_consts: Vec::new(),
            threads,
        })
    }
    pub fn specialize(mut self, spec_consts: &[ScalarElem]) -> Result<Self> {
        assert_eq!(spec_consts.len(), self.desc.spec_descs.len());
        for (spec_const, spec_desc) in spec_consts.iter().copied().zip(self.desc.spec_descs.iter())
        {
            assert_eq!(spec_const.scalar_type(), spec_desc.scalar_type);
            if let Some(dim) = spec_desc.thread_dim {
                if let ScalarElem::U32(value) = spec_const {
                    if value == 0 {
                        bail!("threads.{} cannot be zero!", ["x", "y", "z"][dim],);
                    }
                    self.threads[dim] = value;
                } else {
                    unreachable!()
                }
            }
        }
        self.spec_consts.clear();
        self.spec_consts.extend_from_slice(spec_consts);
        Ok(self)
    }
    pub fn features(&self) -> Features {
        self.desc.features
    }
    pub fn hash(&self) -> u64 {
        self.desc.hash
    }
    pub fn safe(&self) -> bool {
        self.desc.safe
    }
    pub fn build(&self, device: Device) -> Result<Kernel> {
        match device.inner {
            DeviceInner::Host => {
                bail!("Kernel `{}` expected device, found host!", self.desc.name);
            }
            #[cfg(feature = "device")]
            DeviceInner::Device(device) => {
                let desc = &self.desc;
                let spec_bytes = if !self.desc.spec_descs.is_empty() {
                    if self.spec_consts.is_empty() {
                        bail!("Kernel `{}` must be specialized!", self.desc.name);
                    }
                    self.spec_consts
                        .iter()
                        .flat_map(|x| x.as_bytes())
                        .copied()
                        .collect()
                } else {
                    Vec::new()
                };
                let key = KernelKey {
                    id: self.id,
                    spec_bytes,
                };
                let inner = if !desc.spec_descs.is_empty() {
                    <<Engine as DeviceEngine>::Kernel>::cached(device.engine, key, || {
                        desc.specialize(
                            self.threads[..self.desc.threads.len()].to_vec(),
                            &self.spec_consts,
                        )
                        .map(Arc::new)
                    })?
                } else {
                    <<Engine as DeviceEngine>::Kernel>::cached(device.engine, key, || {
                        Ok(desc.clone())
                    })?
                };
                Ok(Kernel {
                    inner,
                    groups: None,
                })
            }
        }
    }
}

#[cfg(feature = "device")]
#[derive(PartialEq, Eq, Hash, Debug)]
struct KernelKey {
    id: usize,
    spec_bytes: Vec<u8>,
}

#[doc(hidden)]
#[derive(Clone)]
pub struct Kernel {
    #[cfg(feature = "device")]
    inner: Arc<<Engine as DeviceEngine>::Kernel>,
    #[cfg(feature = "device")]
    groups: Option<[u32; 3]>,
}

#[cfg(feature = "device")]
fn global_threads_to_groups(global_threads: &[u32], threads: &[u32]) -> [u32; 3] {
    debug_assert_eq!(global_threads.len(), threads.len());
    let mut groups = [1; 3];
    for (gt, (g, t)) in global_threads
        .iter()
        .copied()
        .zip(groups.iter_mut().zip(threads.iter().copied()))
    {
        *g = gt / t + if gt % t != 0 { 1 } else { 0 };
    }
    groups
}

impl Kernel {
    pub fn global_threads(
        #[cfg_attr(not(feature = "device"), allow(unused_mut))] mut self,
        global_threads: &[u32],
    ) -> Self {
        #[cfg(feature = "device")]
        {
            let desc = &self.inner.desc();
            let groups = global_threads_to_groups(global_threads, &desc.threads);
            self.groups.replace(groups);
            self
        }
        #[cfg(not(feature = "device"))]
        {
            let _ = global_threads;
            unreachable!()
        }
    }
    pub fn groups(
        #[cfg_attr(not(feature = "device"), allow(unused_mut))] mut self,
        groups: &[u32],
    ) -> Self {
        #[cfg(feature = "device")]
        {
            debug_assert_eq!(groups.len(), self.inner.desc().threads.len());
            let mut new_groups = [1; 3];
            new_groups[..groups.len()].copy_from_slice(groups);
            self.groups.replace(new_groups);
            self
        }
        #[cfg(not(feature = "device"))]
        {
            let _ = groups;
            unreachable!()
        }
    }
    pub unsafe fn dispatch(
        &self,
        slices: &[KernelSliceArg],
        push_consts: &[ScalarElem],
    ) -> Result<()> {
        #[cfg(feature = "device")]
        {
            let desc = &self.inner.desc();
            let kernel_name = &desc.name;
            let mut buffers = Vec::with_capacity(desc.slice_descs.len());
            let mut items: Option<usize> = None;
            for (slice, slice_desc) in slices.into_iter().zip(desc.slice_descs.iter()) {
                debug_assert_eq!(slice.scalar_type(), slice_desc.scalar_type);
                debug_assert!(!slice_desc.mutable || slice.mutable());
                let slice_name = &slice_desc.name;
                let buffer = if let Some(buffer) = slice.device_buffer() {
                    buffer
                } else {
                    bail!("Kernel `{kernel_name}`.`{slice_name}` expected device, found host!");
                };
                if !Arc::ptr_eq(buffer.inner.engine(), self.inner.engine()) {
                    let device = RawDevice {
                        engine: self.inner.engine().clone(),
                    };
                    let buffer_device = buffer.device();
                    bail!(
                        "Kernel `{kernel_name}`.`{slice_name}`, expected `{device:?}`, found {buffer_device:?}!"
                    );
                }
                buffers.push(buffer.inner.clone());
                if slice_desc.item {
                    items.replace(if let Some(items) = items {
                        items.min(slice.len())
                    } else {
                        slice.len()
                    });
                }
            }
            let groups = if let Some(groups) = self.groups {
                groups
            } else if let Some(items) = items {
                if desc.threads.iter().skip(1).any(|t| *t > 1) {
                    bail!("Kernel `{kernel_name}` cannot infer global_threads if threads.y > 1 or threads.z > 1, threads = {threads:?}!", threads = desc.threads);
                }
                global_threads_to_groups(&[items as u32], &[desc.threads[0]])
            } else {
                bail!("Kernel `{kernel_name}` global_threads or groups not provided!");
            };
            let mut push_bytes = Vec::with_capacity(desc.push_consts_range() as usize);
            for (push, push_desc) in push_consts.iter().zip(desc.push_descs.iter()) {
                debug_assert_eq!(push.scalar_type(), push_desc.scalar_type);
                push_bytes.extend_from_slice(push.as_bytes());
            }
            unsafe { self.inner.dispatch(groups, &buffers, push_bytes) }
        }
        #[cfg(not(feature = "device"))]
        {
            let _ = (slices, push_consts);
            unreachable!()
        }
    }
    pub fn threads(&self) -> &[u32] {
        #[cfg(feature = "device")]
        {
            return self.inner.desc().threads.as_ref();
        }
        #[cfg(not(feature = "device"))]
        {
            unreachable!()
        }
    }
    pub fn features(&self) -> Features {
        #[cfg(feature = "device")]
        {
            return self.inner.desc().features;
        }
        #[cfg(not(feature = "device"))]
        {
            unreachable!()
        }
    }
}

#[doc(hidden)]
pub enum KernelSliceArg<'a> {
    Slice(ScalarSlice<'a>),
    SliceMut(ScalarSliceMut<'a>),
}

#[cfg(feature = "device")]
impl KernelSliceArg<'_> {
    fn scalar_type(&self) -> ScalarType {
        match self {
            Self::Slice(x) => x.scalar_type(),
            Self::SliceMut(x) => x.scalar_type(),
        }
    }
    fn mutable(&self) -> bool {
        match self {
            Self::Slice(_) => false,
            Self::SliceMut(_) => true,
        }
    }
    /*fn device(&self) -> Device {
        match self {
            Self::Slice(x) => x.device(),
            Self::SliceMut(x) => x.device(),
        }
    }*/
    fn device_buffer(&self) -> Option<&DeviceBuffer> {
        match self {
            Self::Slice(x) => x.device_buffer(),
            Self::SliceMut(x) => x.device_buffer_mut(),
        }
    }
    fn len(&self) -> usize {
        match self {
            Self::Slice(x) => x.len(),
            Self::SliceMut(x) => x.len(),
        }
    }
}

impl<'a, T: Scalar> From<Slice<'a, T>> for KernelSliceArg<'a> {
    fn from(slice: Slice<'a, T>) -> Self {
        Self::Slice(slice.into())
    }
}

impl<'a, T: Scalar> From<SliceMut<'a, T>> for KernelSliceArg<'a> {
    fn from(slice: SliceMut<'a, T>) -> Self {
        Self::SliceMut(slice.into())
    }
}
