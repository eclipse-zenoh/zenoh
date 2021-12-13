use super::*;
pub use no_mangle::*;

pub type PluginVTableVersion = u16;
type LoadPluginResultInner = Result<PluginVTableInner<(), ()>, PluginVTableVersion>;
pub type LoadPluginResult<A, B> = Result<PluginVTable<A, B>, PluginVTableVersion>;

/// This number should change any time the internal structure of [`PluginVTable`] changes
pub const PLUGIN_VTABLE_VERSION: PluginVTableVersion = 1;

type StartFn<StartArgs, RunningPlugin> = fn(&str, &StartArgs) -> ZResult<RunningPlugin>;

#[repr(C)]
struct PluginVTableInner<StartArgs, RunningPlugin> {
    is_compatible_with: fn(&[PluginId]) -> Result<PluginId, Incompatibility>,
    start: StartFn<StartArgs, RunningPlugin>,
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
pub struct PluginVTable<StartArgs, RunningPlugin> {
    inner: PluginVTableInner<StartArgs, RunningPlugin>,
    padding: PluginVTablePadding,
}

impl<StartArgs, RunningPlugin> PluginVTable<StartArgs, RunningPlugin> {
    pub fn new<ConcretePlugin: Plugin<StartArgs = StartArgs, RunningPlugin = RunningPlugin>>(
    ) -> Self {
        PluginVTable {
            inner: PluginVTableInner {
                is_compatible_with: ConcretePlugin::is_compatible_with,
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

    pub fn start(&self, name: &str, start_args: &StartArgs) -> ZResult<RunningPlugin> {
        (self.inner.start)(name, start_args)
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
            ) -> LoadPluginResult<<$ty as Plugin>::StartArgs, <$ty as Plugin>::RunningPlugin> {
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
