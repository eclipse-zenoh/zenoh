//
// Copyright (c) 2023 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//
use std::{borrow::Cow, convert::Infallible, fmt, str::FromStr};

use phf::phf_map;
use zenoh_buffers::ZSlice;
use zenoh_protocol::core::EncodingId;

/// # Encoding information
///
/// The [`Sample`](crate::sample::Sample) or [`ReplyError`](crate::query::ReplyError)
/// may contain the optional encoding value, which indicates how the payload should be interpreted
/// by the application.
///
/// The encoding is represented as a string in MIME-like format: `type/subtype[;schema]`.
///
/// To optimize network usage, Zenoh internally maps some predefined encoding strings to
/// an integer identifier. These encodings are provided as associated constants of the [`Encoding`] struct,
/// e.g. [`Encoding::ZENOH_BYTES`], [`Encoding::APPLICATION_JSON`], etc. This internal mapping
/// is not exposed to the application layer and, from the API point of view, the encoding is always
/// represented as a string. But it's useful to know that some encodings are much more efficient
/// than others, especially considering that the encoding value is sent with each message.
///
/// Please note that the Zenoh protocol does not impose any encoding value, nor does it operate on it.
/// It can be seen as optional metadata that is carried over by Zenoh in such a way that the application may perform different operations depending on the encoding value.
///
/// # Examples
///
/// ### String operations
///
/// Create an [`Encoding`] from a string and vice versa.
/// ```
/// use zenoh::bytes::Encoding;
///
/// let encoding: Encoding = "text/plain".into();
/// let text: String = encoding.clone().into();
/// assert_eq!("text/plain", &text);
/// ```
///
/// ### Constants and cow operations
///
/// Since some encoding values are internally optimized by Zenoh, it's generally more efficient to use
/// the defined constants and [`Cow`][std::borrow::Cow] conversion to obtain its string representation.
/// ```
/// use zenoh::bytes::Encoding;
/// use std::borrow::Cow;
///
/// // This allocates
/// assert_eq!("text/plain", &String::from(Encoding::TEXT_PLAIN));
/// // This does NOT allocate
/// assert_eq!("text/plain", &Cow::from(Encoding::TEXT_PLAIN));
/// ```
///
/// ### Schema
///
/// Additionally, a schema can be associated with the encoding.
/// The convention is to use the `;` separator if an encoding is created from a string.
/// Alternatively, [`with_schema()`](Encoding::with_schema) can be used to add a schema to one of the associated constants.
/// ```
/// use zenoh::bytes::Encoding;
///
/// let encoding1 = Encoding::from("text/plain;utf-8");
/// let encoding2 = Encoding::TEXT_PLAIN.with_schema("utf-8");
/// assert_eq!(encoding1, encoding2);
/// assert_eq!("text/plain;utf-8", &encoding1.to_string());
/// assert_eq!("text/plain;utf-8", &encoding2.to_string());
/// ```
#[repr(transparent)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Encoding(zenoh_protocol::core::Encoding);

impl Encoding {
    const SCHEMA_SEP: char = ';';
    const CUSTOM_ENCODING_ID: u16 = 0xFFFFu16;

    // For compatibility purposes, Zenoh reserves any prefix value from `0` to `1023`, inclusive.

    // - Primitive types supported in all Zenoh bindings
    /// Just some bytes.
    ///
    /// Constant alias for string: `"zenoh/bytes"`.
    ///
    /// This encoding assumes that the payload was created with [`ZBytes::from::<Vec<u8>>`](crate::bytes::ZBytes::from) or similar
    /// (`[u8]`, `[u8;N]`, `Cow<_,[u8]>`) and its data can be accessed with [`ZBytes::to_bytes()`](crate::bytes::ZBytes::to_bytes),
    /// no additional assumptions about data format are made.
    pub const ZENOH_BYTES: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 0,
        schema: None,
    });
    /// A UTF-8 string.
    ///
    /// Constant alias for string: `"zenoh/string"`.
    ///
    /// This encoding assumes that the payload was created with [`ZBytes::from::<String>`](crate::bytes::ZBytes::from) or similar
    /// (`&str`, `Cow<str>`) and its data can be accessed with [`ZBytes::try_to_string()`](crate::bytes::ZBytes::try_to_string) without an error.
    pub const ZENOH_STRING: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 1,
        schema: None,
    });

    /// Zenoh serialized data.
    ///
    /// Constant alias for string: `"zenoh/serialized"`.
    ///
    /// This encoding assumes that the payload was created with serialization functions provided by the `zenoh-ext` crate.
    /// The `schema` field may contain the details of the serialization format.
    pub const ZENOH_SERIALIZED: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 2,
        schema: None,
    });
    // - Advanced types may be supported in some of the Zenoh bindings.
    /// An application-specific stream of bytes.
    ///
    /// Constant alias for string: `"application/octet-stream"`.
    pub const APPLICATION_OCTET_STREAM: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 3,
        schema: None,
    });
    /// A textual file.
    ///
    /// Constant alias for string: `"text/plain"`.
    pub const TEXT_PLAIN: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 4,
        schema: None,
    });
    /// JSON data intended to be consumed by an application.
    ///
    /// Constant alias for string: `"application/json"`.
    pub const APPLICATION_JSON: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 5,
        schema: None,
    });
    /// JSON data intended to be human readable.
    ///
    /// Constant alias for string: `"text/json"`.
    pub const TEXT_JSON: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 6,
        schema: None,
    });
    /// Common Data Representation (CDR)-encoded data.
    ///
    /// Constant alias for string: `"application/cdr"`.
    pub const APPLICATION_CDR: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 7,
        schema: None,
    });
    /// Concise Binary Object Representation (CBOR)-encoded data.
    ///
    /// Constant alias for string: `"application/cbor"`.
    pub const APPLICATION_CBOR: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 8,
        schema: None,
    });
    /// YAML data intended to be consumed by an application.
    ///
    /// Constant alias for string: `"application/yaml"`.
    pub const APPLICATION_YAML: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 9,
        schema: None,
    });
    /// YAML data intended to be human readable.
    ///
    /// Constant alias for string: `"text/yaml"`.
    pub const TEXT_YAML: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 10,
        schema: None,
    });
    /// JSON5-encoded data that are human readable.
    ///
    /// Constant alias for string: `"text/json5"`.
    pub const TEXT_JSON5: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 11,
        schema: None,
    });
    /// A Python object serialized using [pickle](https://docs.python.org/3/library/pickle.html).
    ///
    /// Constant alias for string: `"application/python-serialized-object"`.
    pub const APPLICATION_PYTHON_SERIALIZED_OBJECT: Encoding =
        Self(zenoh_protocol::core::Encoding {
            id: 12,
            schema: None,
        });
    /// Application-specific protobuf-encoded data.
    ///
    /// Constant alias for string: `"application/protobuf"`.
    pub const APPLICATION_PROTOBUF: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 13,
        schema: None,
    });
    /// A Java serialized object.
    ///
    /// Constant alias for string: `"application/java-serialized-object"`.
    pub const APPLICATION_JAVA_SERIALIZED_OBJECT: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 14,
        schema: None,
    });
    /// [OpenMetrics](https://github.com/OpenObservability/OpenMetrics) data, commonly used by [Prometheus](https://prometheus.io/).
    ///
    /// Constant alias for string: `"application/openmetrics-text"`.
    pub const APPLICATION_OPENMETRICS_TEXT: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 15,
        schema: None,
    });
    /// A Portable Network Graphics (PNG) image.
    ///
    /// Constant alias for string: `"image/png"`.
    pub const IMAGE_PNG: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 16,
        schema: None,
    });
    /// A Joint Photographic Experts Group (JPEG) image.
    ///
    /// Constant alias for string: `"image/jpeg"`.
    pub const IMAGE_JPEG: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 17,
        schema: None,
    });
    /// A Graphics Interchange Format (GIF) image.
    ///
    /// Constant alias for string: `"image/gif"`.
    pub const IMAGE_GIF: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 18,
        schema: None,
    });
    /// A Bitmap (BMP) image.
    ///
    /// Constant alias for string: `"image/bmp"`.
    pub const IMAGE_BMP: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 19,
        schema: None,
    });
    /// A WebP image.
    ///
    ///  Constant alias for string: `"image/webp"`.
    pub const IMAGE_WEBP: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 20,
        schema: None,
    });
    /// An XML file intended to be consumed by an application.
    ///
    /// Constant alias for string: `"application/xml"`.
    pub const APPLICATION_XML: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 21,
        schema: None,
    });
    /// An encoded list of tuples, each consisting of a name and a value.
    ///
    /// Constant alias for string: `"application/x-www-form-urlencoded"`.
    pub const APPLICATION_X_WWW_FORM_URLENCODED: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 22,
        schema: None,
    });
    /// An HTML file.
    ///
    /// Constant alias for string: `"text/html"`.
    pub const TEXT_HTML: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 23,
        schema: None,
    });
    /// An XML file that is human readable.
    ///
    /// Constant alias for string: `"text/xml"`.
    pub const TEXT_XML: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 24,
        schema: None,
    });
    /// A CSS file.
    ///
    /// Constant alias for string: `"text/css"`.
    pub const TEXT_CSS: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 25,
        schema: None,
    });
    /// A JavaScript file.
    ///
    /// Constant alias for string: `"text/javascript"`.
    pub const TEXT_JAVASCRIPT: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 26,
        schema: None,
    });
    /// A Markdown file.
    ///
    /// Constant alias for string: `"text/markdown"`.
    pub const TEXT_MARKDOWN: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 27,
        schema: None,
    });
    /// A CSV file.
    ///
    /// Constant alias for string: `"text/csv"`.
    pub const TEXT_CSV: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 28,
        schema: None,
    });
    /// An application-specific SQL query.
    ///
    /// Constant alias for string: `"application/sql"`.
    pub const APPLICATION_SQL: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 29,
        schema: None,
    });
    /// Constrained Application Protocol (CoAP) data intended for CoAP-to-HTTP and HTTP-to-CoAP proxies.
    ///
    /// Constant alias for string: `"application/coap-payload"`.
    pub const APPLICATION_COAP_PAYLOAD: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 30,
        schema: None,
    });
    /// Defines a JSON document structure for expressing a sequence of operations to apply to a JSON document.
    ///
    /// Constant alias for string: `"application/json-patch+json"`.
    pub const APPLICATION_JSON_PATCH_JSON: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 31,
        schema: None,
    });
    /// A JSON text sequence consists of any number of JSON texts, all encoded in UTF-8.
    ///
    /// Constant alias for string: `"application/json-seq"`.
    pub const APPLICATION_JSON_SEQ: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 32,
        schema: None,
    });
    /// JSONPath defines a string syntax for selecting and extracting JSON values from within a given JSON value.
    ///
    /// Constant alias for string: `"application/jsonpath"`.
    pub const APPLICATION_JSONPATH: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 33,
        schema: None,
    });
    /// A JSON Web Token (JWT).
    ///
    /// Constant alias for string: `"application/jwt"`.
    pub const APPLICATION_JWT: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 34,
        schema: None,
    });
    /// Application-specific MPEG-4-encoded data, either audio or video.
    ///
    /// Constant alias for string: `"application/mp4"`.
    pub const APPLICATION_MP4: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 35,
        schema: None,
    });
    /// A SOAP 1.2 message serialized as XML 1.0.
    ///
    /// Constant alias for string: `"application/soap+xml"`.
    pub const APPLICATION_SOAP_XML: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 36,
        schema: None,
    });
    /// YANG-encoded data commonly used by the Network Configuration Protocol (NETCONF).
    ///
    /// Constant alias for string: `"application/yang"`.
    pub const APPLICATION_YANG: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 37,
        schema: None,
    });
    /// An MPEG-4 Advanced Audio Coding (AAC) media.
    ///
    /// Constant alias for string: `"audio/aac"`.
    pub const AUDIO_AAC: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 38,
        schema: None,
    });
    /// A Free Lossless Audio Codec (FLAC) media.
    ///
    /// Constant alias for string: `"audio/flac"`.
    pub const AUDIO_FLAC: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 39,
        schema: None,
    });
    /// An audio codec defined in MPEG-1, MPEG-2, MPEG-4, or registered at the MP4 registration authority.
    ///
    /// Constant alias for string: `"audio/mp4"`.
    pub const AUDIO_MP4: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 40,
        schema: None,
    });
    /// An Ogg-encapsulated audio stream.
    ///
    /// Constant alias for string: `"audio/ogg"`.
    pub const AUDIO_OGG: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 41,
        schema: None,
    });
    /// A Vorbis-encoded audio stream.
    ///
    /// Constant alias for string: `"audio/vorbis"`.
    pub const AUDIO_VORBIS: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 42,
        schema: None,
    });
    /// An h261-encoded video stream.
    ///
    /// Constant alias for string: `"video/h261"`.
    pub const VIDEO_H261: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 43,
        schema: None,
    });
    /// An h263-encoded video stream.
    ///
    /// Constant alias for string: `"video/h263"`.
    pub const VIDEO_H263: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 44,
        schema: None,
    });
    /// An h264-encoded video stream.
    ///
    /// Constant alias for string: `"video/h264"`.
    pub const VIDEO_H264: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 45,
        schema: None,
    });
    /// An h265-encoded video stream.
    ///
    /// Constant alias for string: `"video/h265"`.
    pub const VIDEO_H265: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 46,
        schema: None,
    });
    /// An h266-encoded video stream.
    ///
    /// Constant alias for string: `"video/h266"`.
    pub const VIDEO_H266: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 47,
        schema: None,
    });
    /// A video codec defined in MPEG-1, MPEG-2, MPEG-4, or registered at the MP4 registration authority.
    ///
    /// Constant alias for string: `"video/mp4"`.
    pub const VIDEO_MP4: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 48,
        schema: None,
    });
    /// An Ogg-encapsulated video stream.
    ///
    /// Constant alias for string: `"video/ogg"`.
    pub const VIDEO_OGG: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 49,
        schema: None,
    });
    /// An uncompressed, studio-quality video stream.
    ///
    /// Constant alias for string: `"video/raw"`.
    pub const VIDEO_RAW: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 50,
        schema: None,
    });
    /// A VP8-encoded video stream.
    ///
    /// Constant alias for string: `"video/vp8"`.
    pub const VIDEO_VP8: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 51,
        schema: None,
    });
    /// A VP9-encoded video stream.
    ///
    /// Constant alias for string: `"video/vp9"`.
    pub const VIDEO_VP9: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 52,
        schema: None,
    });

    const ID_TO_STR: phf::Map<EncodingId, &'static str> = phf_map! {
        0u16 => "zenoh/bytes",
        1u16 => "zenoh/string",
        2u16 => "zenoh/serialized",
        3u16 => "application/octet-stream",
        4u16 => "text/plain",
        5u16 => "application/json",
        6u16 => "text/json",
        7u16 => "application/cdr",
        8u16 => "application/cbor",
        9u16 => "application/yaml",
        10u16 => "text/yaml",
        11u16 => "text/json5",
        12u16 => "application/python-serialized-object",
        13u16 => "application/protobuf",
        14u16 => "application/java-serialized-object",
        15u16 => "application/openmetrics-text",
        16u16 => "image/png",
        17u16 => "image/jpeg",
        18u16 => "image/gif",
        19u16 => "image/bmp",
        20u16 => "image/webp",
        21u16 => "application/xml",
        22u16 => "application/x-www-form-urlencoded",
        23u16 => "text/html",
        24u16 => "text/xml",
        25u16 => "text/css",
        26u16 => "text/javascript",
        27u16 => "text/markdown",
        28u16 => "text/csv",
        29u16 => "application/sql",
        30u16 => "application/coap-payload",
        31u16 => "application/json-patch+json",
        32u16 => "application/json-seq",
        33u16 => "application/jsonpath",
        34u16 => "application/jwt",
        35u16 => "application/mp4",
        36u16 => "application/soap+xml",
        37u16 => "application/yang",
        38u16 => "audio/aac",
        39u16 => "audio/flac",
        40u16 => "audio/mp4",
        41u16 => "audio/ogg",
        42u16 => "audio/vorbis",
        43u16 => "video/h261",
        44u16 => "video/h263",
        45u16 => "video/h264",
        46u16 => "video/h265",
        47u16 => "video/h266",
        48u16 => "video/mp4",
        49u16 => "video/ogg",
        50u16 => "video/raw",
        51u16 => "video/vp8",
        52u16 => "video/vp9",
        // The 0xFFFFu16 is used to indicate a custom encoding where both encoding and schema
        // are stored in the schema field.
        0xFFFFu16 => "",
    };

    const STR_TO_ID: phf::Map<&'static str, EncodingId> = phf_map! {
        "zenoh/bytes" => 0u16,
        "zenoh/string" => 1u16,
        "zenoh/serialized" => 2u16,
        "application/octet-stream" => 3u16,
        "text/plain" => 4u16,
        "application/json" => 5u16,
        "text/json" => 6u16,
        "application/cdr" => 7u16,
        "application/cbor" => 8u16,
        "application/yaml" => 9u16,
        "text/yaml" => 10u16,
        "text/json5" => 11u16,
        "application/python-serialized-object" => 12u16,
        "application/protobuf" => 13u16,
        "application/java-serialized-object" => 14u16,
        "application/openmetrics-text" => 15u16,
        "image/png" => 16u16,
        "image/jpeg" => 17u16,
        "image/gif" => 18u16,
        "image/bmp" => 19u16,
        "image/webp" => 20u16,
        "application/xml" => 21u16,
        "application/x-www-form-urlencoded" => 22u16,
        "text/html" => 23u16,
        "text/xml" => 24u16,
        "text/css" => 25u16,
        "text/javascript" => 26u16,
        "text/markdown" => 27u16,
        "text/csv" => 28u16,
        "application/sql" => 29u16,
        "application/coap-payload" => 30u16,
        "application/json-patch+json" => 31u16,
        "application/json-seq" => 32u16,
        "application/jsonpath" => 33u16,
        "application/jwt" => 34u16,
        "application/mp4" => 35u16,
        "application/soap+xml" => 36u16,
        "application/yang" => 37u16,
        "audio/aac" => 38u16,
        "audio/flac" => 39u16,
        "audio/mp4" => 40u16,
        "audio/ogg" => 41u16,
        "audio/vorbis" => 42u16,
        "video/h261" => 43u16,
        "video/h263" => 44u16,
        "video/h264" => 45u16,
        "video/h265" => 46u16,
        "video/h266" => 47u16,
        "video/mp4" => 48u16,
        "video/ogg" => 49u16,
        "video/raw" => 50u16,
        "video/vp8" => 51u16,
        "video/vp9" => 52u16,
    };

    /// The default [`Encoding`] is [`ZENOH_BYTES`](Encoding::ZENOH_BYTES).
    pub const fn default() -> Self {
        Self::ZENOH_BYTES
    }

    /// Set a schema for this encoding. Zenoh does not define what a schema is, and its semantics is left to the implementer.
    /// E.g., a common schema for the `text/plain` encoding is `utf-8`.
    pub fn with_schema<S>(mut self, s: S) -> Self
    where
        S: Into<String>,
    {
        let s: String = s.into();
        let schema_str = match self.0.id {
            Self::CUSTOM_ENCODING_ID => {
                let schema_str = self
                    .0
                    .schema
                    .as_ref()
                    .map(|schema| std::str::from_utf8(schema).unwrap_or(""))
                    .unwrap_or("");
                let (encoding, _) = schema_str
                    .split_once(Self::SCHEMA_SEP)
                    .unwrap_or((schema_str, ""));
                match (encoding.is_empty(), s.is_empty()) {
                    (true, _) => s,
                    (false, true) => encoding.to_string(),
                    (false, false) => format!("{encoding}{}{s}", Self::SCHEMA_SEP),
                }
            }
            _ => s,
        };
        self.0.schema = Some(schema_str.into_boxed_str().into_boxed_bytes().into());
        self
    }
}

impl Default for Encoding {
    fn default() -> Self {
        Self::default()
    }
}

impl From<&str> for Encoding {
    fn from(t: &str) -> Self {
        let mut inner = zenoh_protocol::core::Encoding::empty();

        // Check if empty
        if t.is_empty() {
            return Encoding(inner);
        }

        // Everything before `;` may be mapped to a known id
        let (id, mut schema) = t.split_once(Encoding::SCHEMA_SEP).unwrap_or((t, ""));
        if let Some(id) = Encoding::STR_TO_ID.get(id).copied() {
            inner.id = id;
        // if id is not recognized, e.g. `t == "my_encoding"`, put it in the schema
        // and set the id to CUSTOM_ENCODING_ID (0xFFFF)
        } else {
            inner.id = Self::CUSTOM_ENCODING_ID;
            schema = t;
        }
        if !schema.is_empty() {
            inner.schema = Some(ZSlice::from(schema.to_string().into_bytes()));
        }

        Encoding(inner)
    }
}

impl From<String> for Encoding {
    fn from(value: String) -> Self {
        Self::from(value.as_str())
    }
}

impl FromStr for Encoding {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::from(s))
    }
}

impl From<&Encoding> for Cow<'static, str> {
    fn from(encoding: &Encoding) -> Self {
        fn su8_to_str(schema: &[u8]) -> &str {
            std::str::from_utf8(schema).unwrap_or("unknown(non-utf8)")
        }

        match (
            Encoding::ID_TO_STR.get(&encoding.0.id).copied(),
            encoding.0.schema.as_ref(),
        ) {
            // Perfect match
            (Some(i), None) => Cow::Borrowed(i),
            // Custom encoding, both id and schema are in the schema field
            (Some(""), Some(s)) => Cow::Owned(su8_to_str(s).into()),
            // ID and schema
            (Some(i), Some(s)) => {
                Cow::Owned(format!("{}{}{}", i, Encoding::SCHEMA_SEP, su8_to_str(s)))
            }
            //
            (None, Some(s)) => Cow::Owned(format!(
                "unknown({}){}{}",
                encoding.0.id,
                Encoding::SCHEMA_SEP,
                su8_to_str(s)
            )),
            (None, None) => Cow::Owned(format!("unknown({})", encoding.0.id)),
        }
    }
}

impl From<Encoding> for Cow<'static, str> {
    fn from(encoding: Encoding) -> Self {
        Self::from(&encoding)
    }
}

impl From<Encoding> for String {
    fn from(encoding: Encoding) -> Self {
        encoding.to_string()
    }
}

impl From<Encoding> for zenoh_protocol::core::Encoding {
    fn from(value: Encoding) -> Self {
        value.0
    }
}

impl From<zenoh_protocol::core::Encoding> for Encoding {
    fn from(value: zenoh_protocol::core::Encoding) -> Self {
        Self(value)
    }
}

impl fmt::Display for Encoding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::fmt::Result {
        let s = Cow::from(self);
        f.write_str(s.as_ref())
    }
}

impl Encoding {
    #[zenoh_macros::internal]
    pub fn id(&self) -> EncodingId {
        self.0.id
    }
    #[zenoh_macros::internal]
    pub fn schema(&self) -> Option<&ZSlice> {
        self.0.schema.as_ref()
    }
    #[zenoh_macros::internal]
    pub fn new(id: EncodingId, schema: Option<ZSlice>) -> Self {
        Encoding(zenoh_protocol::core::Encoding { id, schema })
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::hash_map::DefaultHasher,
        hash::{Hash, Hasher},
    };

    use super::*;

    #[test]
    fn hash() {
        let encodings = [
            Encoding::ZENOH_BYTES,
            Encoding::ZENOH_STRING,
            Encoding::ZENOH_STRING.with_schema("utf-8"),
            Encoding::ZENOH_STRING.with_schema("utf-8"),
        ];
        let hashes = encodings.map(|e| {
            let mut hasher = DefaultHasher::new();
            e.hash(&mut hasher);
            hasher.finish()
        });

        assert_ne!(hashes[0], hashes[1]);
        assert_ne!(hashes[0], hashes[2]);
        assert_ne!(hashes[1], hashes[2]);
        assert_eq!(hashes[2], hashes[3]);
    }

    #[test]
    fn test_default_encoding() {
        let default = Encoding::default();
        assert_eq!(default, Encoding::ZENOH_BYTES);
        assert_eq!(default.to_string(), "zenoh/bytes");
    }

    #[test]
    fn test_from_str() {
        // Test constants conversion
        assert_eq!(Encoding::from("zenoh/bytes"), Encoding::ZENOH_BYTES);
        assert_eq!(Encoding::from("zenoh/string"), Encoding::ZENOH_STRING);
        assert_eq!(Encoding::from("text/plain"), Encoding::TEXT_PLAIN);
        assert_eq!(
            Encoding::from("application/json"),
            Encoding::APPLICATION_JSON
        );
    }

    #[test]
    fn test_to_string() {
        // Test constants to string
        assert_eq!(Encoding::ZENOH_BYTES.to_string(), "zenoh/bytes");
        assert_eq!(Encoding::ZENOH_STRING.to_string(), "zenoh/string");
        assert_eq!(Encoding::TEXT_PLAIN.to_string(), "text/plain");
        assert_eq!(Encoding::APPLICATION_JSON.to_string(), "application/json");
    }

    #[test]
    fn test_schema() {
        // Test with schema using with_schema method
        let with_schema = Encoding::TEXT_PLAIN.with_schema("utf-8");
        assert_eq!(with_schema.to_string(), "text/plain;utf-8");

        // Test from string with schema
        let from_str = Encoding::from("text/plain;utf-8");
        assert_eq!(from_str.to_string(), "text/plain;utf-8");

        // Unknown encoding with schema
        let unknown_with_schema = Encoding::from("custom/format;v1.0");
        assert_eq!(unknown_with_schema.to_string(), "custom/format;v1.0");
        let unknown_with_schema = unknown_with_schema.with_schema("v2.0");
        assert_eq!(unknown_with_schema.to_string(), "custom/format;v2.0");
    }
}
