use super::*;
pub use no_mangle::*;

pub type PluginVTableVersion = u16;
type LoadPluginResultInner = Result<PluginVTableInner<(), ()>, PluginVTableVersion>;
pub type LoadPluginResult<A, B> = Result<PluginVTable<A, B>, PluginVTableVersion>;

/// This number should change any time the internal structure of [`PluginVTable`] changes
pub const PLUGIN_VTABLE_VERSION: PluginVTableVersion = 0;

#[repr(C)]
struct PluginVTableInner<Requirements, StartArgs> {
    is_compatible_with: fn(&[PluginId]) -> Result<PluginId, Incompatibility>,
    get_requirements: fn() -> Requirements,
    start: fn(&StartArgs) -> Result<BoxedAny, Box<dyn Error>>,
}

/// Automagical padding such that [PluginVTable::init]'s result is the size of a cache line
#[repr(C)]
struct PluginVTablePadding {
    __padding: [u8; PluginVTablePadding::padding_length()],
}
impl PluginVTablePadding {
    const fn padding_length() -> usize {
        64 - std::mem::size_of::<LoadPluginResultInner>()
    }
    fn new() -> Self {
        PluginVTablePadding {
            __padding: [0; Self::padding_length()],
        }
    }
}

/// For use with dynamically loaded plugins. Its size will not change accross versions, but its internal structure might.
///
/// To ensure compatibility, its size and alignment must allow `size_of::<Result<PluginVTable, PluginVTableVersion>>() == 64` (one cache line).
#[repr(C)]
pub struct PluginVTable<Requirements, StartArgs> {
    inner: PluginVTableInner<Requirements, StartArgs>,
    padding: PluginVTablePadding,
}

impl<Requirements, StartArgs> PluginVTable<Requirements, StartArgs> {
    pub fn new<ConcretePlugin: Plugin<Requirements = Requirements, StartArgs = StartArgs>>() -> Self
    {
        PluginVTable {
            inner: PluginVTableInner {
                is_compatible_with: ConcretePlugin::is_compatible_with,
                get_requirements: ConcretePlugin::get_requirements,
                start: ConcretePlugin::start,
            },
            padding: PluginVTablePadding::new(),
        }
    }

    /// Ensures [PluginVTable]'s size stays the same between versions
    fn __size_check() {
        unsafe {
            std::mem::transmute::<_, [u8; 64]>(std::mem::MaybeUninit::<
                Result<Self, PluginVTableVersion>,
            >::uninit())
        };
    }

    pub fn is_compatible_with(&self, others: &[PluginId]) -> Result<PluginId, Incompatibility> {
        (self.inner.is_compatible_with)(others)
    }

    pub fn get_requirements(&self) -> Requirements {
        (self.inner.get_requirements)()
    }

    pub fn start(&self, start_args: &StartArgs) -> Result<BoxedAny, Box<dyn Error>> {
        (self.inner.start)(start_args)
    }
}

pub use no_mangle::*;
#[cfg(feature = "no_mangle")]
pub mod no_mangle {
    /// This macro will add a non-mangled `load_plugin` function to the library if feature `no_mangle` is enabled (which it is by default).
    #[macro_export]
    macro_rules! declare_plugin {
        ($ty: path) => {
            #[no_mangle]
            fn load_plugin(
                version: PluginVTableVersion,
            ) -> LoadPluginResult<<$ty as Plugin>::Requirements, <$ty as Plugin>::StartArgs> {
                if version == PLUGIN_VTABLE_VERSION {
                    Ok(PluginVTable::new::<$ty>())
                } else {
                    Err(PLUGIN_VTABLE_VERSION)
                }
            }
        };
    }
}
#[cfg(not(feature = "no_mangle"))]
pub mod no_mangle {
    #[macro_export]
    macro_rules! declare_plugin {
        ($ty: path) => {};
    }
}
