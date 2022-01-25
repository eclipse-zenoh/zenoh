use core::fmt;
use std::{borrow::Cow, str::FromStr};

use http_types::Mime;
use zenoh_core::{bail, zerror, Result as ZResult};

use crate::ZInt;

/// The encoding of a zenoh [`Value`](crate::Value).
///
/// A zenoh encoding is a [`Mime`](http_types::Mime) type represented, for wire efficiency,
/// as an integer prefix (that maps to a string) and a string suffix.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Encoding {
    pub prefix: ZInt,
    pub suffix: Cow<'static, str>,
}

mod consts {
    use lazy_static::lazy_static;
    lazy_static! {
        pub(super) static ref MIMES: [&'static str; 21] = [
            /*  0 */ "",
            /*  1 */ "application/octet-stream",
            /*  2 */ "application/custom", // non iana standard
            /*  3 */ "text/plain",
            /*  4 */ "application/properties", // non iana standard
            /*  5 */ "application/json", // if not readable from casual users
            /*  6 */ "application/sql",
            /*  7 */ "application/integer", // non iana standard
            /*  8 */ "application/float", // non iana standard
            /*  9 */ "application/xml", // if not readable from casual users (RFC 3023, section 3)
            /* 10 */ "application/xhtml+xml",
            /* 11 */ "application/x-www-form-urlencoded",
            /* 12 */ "text/json", // non iana standard - if readable from casual users
            /* 13 */ "text/html",
            /* 14 */ "text/xml", // if readable from casual users (RFC 3023, section 3)
            /* 15 */ "text/css",
            /* 16 */ "text/csv",
            /* 17 */ "text/javascript",
            /* 18 */ "image/jpeg",
            /* 19 */ "image/png",
            /* 20 */ "image/gif",
        ];
    }
}

impl Encoding {
    pub const EMPTY: Encoding = Encoding {
        prefix: 0,
        suffix: std::borrow::Cow::Borrowed(""),
    };
    pub const APP_OCTET_STREAM: Encoding = Encoding {
        prefix: 1,
        suffix: std::borrow::Cow::Borrowed(""),
    };
    pub const APP_CUSTOM: Encoding = Encoding {
        prefix: 2,
        suffix: std::borrow::Cow::Borrowed(""),
    };
    pub const TEXT_PLAIN: Encoding = Encoding {
        prefix: 3,
        suffix: std::borrow::Cow::Borrowed(""),
    };
    pub const STRING: Encoding = Encoding::TEXT_PLAIN;
    pub const APP_PROPERTIES: Encoding = Encoding {
        prefix: 4,
        suffix: std::borrow::Cow::Borrowed(""),
    };
    pub const APP_JSON: Encoding = Encoding {
        prefix: 5,
        suffix: std::borrow::Cow::Borrowed(""),
    };
    pub const APP_SQL: Encoding = Encoding {
        prefix: 6,
        suffix: std::borrow::Cow::Borrowed(""),
    };
    pub const APP_INTEGER: Encoding = Encoding {
        prefix: 7,
        suffix: std::borrow::Cow::Borrowed(""),
    };
    pub const APP_FLOAT: Encoding = Encoding {
        prefix: 8,
        suffix: std::borrow::Cow::Borrowed(""),
    };
    pub const APP_XML: Encoding = Encoding {
        prefix: 9,
        suffix: std::borrow::Cow::Borrowed(""),
    };
    pub const APP_XHTML_XML: Encoding = Encoding {
        prefix: 10,
        suffix: std::borrow::Cow::Borrowed(""),
    };
    pub const APP_X_WWW_FORM_URLENCODED: Encoding = Encoding {
        prefix: 11,
        suffix: std::borrow::Cow::Borrowed(""),
    };
    pub const TEXT_JSON: Encoding = Encoding {
        prefix: 12,
        suffix: std::borrow::Cow::Borrowed(""),
    };
    pub const TEXT_HTML: Encoding = Encoding {
        prefix: 13,
        suffix: std::borrow::Cow::Borrowed(""),
    };
    pub const TEXT_XML: Encoding = Encoding {
        prefix: 14,
        suffix: std::borrow::Cow::Borrowed(""),
    };
    pub const TEXT_CSS: Encoding = Encoding {
        prefix: 15,
        suffix: std::borrow::Cow::Borrowed(""),
    };
    pub const TEXT_CSV: Encoding = Encoding {
        prefix: 16,
        suffix: std::borrow::Cow::Borrowed(""),
    };
    pub const TEXT_JAVASCRIPT: Encoding = Encoding {
        prefix: 17,
        suffix: std::borrow::Cow::Borrowed(""),
    };
    pub const IMG_JPG: Encoding = Encoding {
        prefix: 18,
        suffix: std::borrow::Cow::Borrowed(""),
    };
    pub const IMG_PNG: Encoding = Encoding {
        prefix: 19,
        suffix: std::borrow::Cow::Borrowed(""),
    };
    pub const IMG_GIF: Encoding = Encoding {
        prefix: 20,
        suffix: std::borrow::Cow::Borrowed(""),
    };

    /// Converts the given encoding to [`Mime`](http_types::Mime).
    pub fn to_mime(&self) -> ZResult<Mime> {
        if self.prefix == 0 {
            Mime::from_str(self.suffix.as_ref()).map_err(|e| zerror!("{}", &e).into())
        } else if self.prefix <= consts::MIMES.len() as ZInt {
            Mime::from_str(&format!(
                "{}{}",
                &consts::MIMES[self.prefix as usize],
                self.suffix
            ))
            .map_err(|e| zerror!("{}", &e).into())
        } else {
            bail!("Unknown encoding prefix {}", self.prefix)
        }
    }

    /// Sets the suffix of this encoding.
    pub fn with_suffix<IntoCowStr>(mut self, suffix: IntoCowStr) -> Self
    where
        IntoCowStr: Into<Cow<'static, str>>,
    {
        self.suffix = suffix.into();
        self
    }

    /// Returns `true`if the string representation of this encoding starts with
    /// the string representation of ther given encoding.
    pub fn starts_with(&self, encoding: &Encoding) -> bool {
        (self.prefix == encoding.prefix && self.suffix.starts_with(encoding.suffix.as_ref()))
            || self.to_string().starts_with(&encoding.to_string())
    }
}

impl fmt::Display for Encoding {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.prefix > 0 && self.prefix < consts::MIMES.len() as ZInt {
            write!(f, "{}{}", &consts::MIMES[self.prefix as usize], self.suffix)
        } else {
            write!(f, "{}", self.suffix)
        }
    }
}

impl From<&'static str> for Encoding {
    fn from(s: &'static str) -> Self {
        for (i, v) in consts::MIMES.iter().enumerate() {
            if i != 0 && s.starts_with(v) {
                return Encoding {
                    prefix: i as ZInt,
                    suffix: s.split_at(v.len()).1.into(),
                };
            }
        }
        Encoding {
            prefix: 0,
            suffix: s.into(),
        }
    }
}

impl<'a> From<String> for Encoding {
    fn from(s: String) -> Self {
        for (i, v) in consts::MIMES.iter().enumerate() {
            if i != 0 && s.starts_with(v) {
                return Encoding {
                    prefix: i as ZInt,
                    suffix: s.split_at(v.len()).1.to_string().into(),
                };
            }
        }
        Encoding {
            prefix: 0,
            suffix: s.into(),
        }
    }
}

impl<'a> From<Mime> for Encoding {
    fn from(m: Mime) -> Self {
        Encoding::from(&m)
    }
}

impl<'a> From<&Mime> for Encoding {
    fn from(m: &Mime) -> Self {
        Encoding::from(m.essence().to_string())
    }
}

impl From<ZInt> for Encoding {
    fn from(i: ZInt) -> Self {
        Encoding {
            prefix: i,
            suffix: "".into(),
        }
    }
}

impl Default for Encoding {
    fn default() -> Self {
        Encoding::EMPTY
    }
}
