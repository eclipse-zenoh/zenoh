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
use zenoh_buffers::{ZBuf, ZSlice};
use zenoh_protocol::core::EncodingId;
#[cfg(feature = "shared-memory")]
use zenoh_shm::api::buffer::{zshm::ZShm, zshmmut::ZShmMut};

use super::bytes::ZBytes;

/// Default encoding values used by Zenoh.
///
/// An encoding has a similar role to *Content-type* in HTTP: it indicates, when present, how data should be interpreted by the application.
///
/// Please note the Zenoh protocol does not impose any encoding value nor it operates on it.
/// It can be seen as some optional metadata that is carried over by Zenoh in such a way the application may perform different operations depending on the encoding value.
///
/// A set of associated constants are provided to cover the most common encodings for user convenience.
/// This is parcticular useful in helping Zenoh to perform additional wire-level optimizations.
///
/// # Examples
///
/// ### String operations
///
/// Create an [`Encoding`] from a string and viceversa.
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
/// Additionally, a schema can be associated to the encoding.
/// The conventions is to use the `;` separator if an encoding is created from a string.
/// Alternatively, [`with_schema()`](Encoding::with_schema) can be used to add a scheme to one of the associated constants.
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Encoding(zenoh_protocol::core::Encoding);

impl Encoding {
    const SCHEMA_SEP: char = ';';

    // For compatibility purposes Zenoh reserves any prefix value from `0` to `1023` included.

    // - Primitives types supported in all Zenoh bindings
    /// Just some bytes.
    ///
    /// Constant alias for string: `"zenoh/bytes"`.
    ///
    /// Usually used for types: `Vec<u8>`, `&[u8]`, `[u8; N]`, `Cow<[u8]>`.
    pub const ZENOH_BYTES: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 0,
        schema: None,
    });
    /// A VLE-encoded signed little-endian 8bit integer. Binary representation uses two's complement.
    ///
    /// Constant alias for string: `"zenoh/int8"`.
    ///
    /// Usually used for types: `i8`.
    pub const ZENOH_INT8: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 1,
        schema: None,
    });
    /// A VLE-encoded signed little-endian 16bit integer. Binary representation uses two's complement.
    ///
    /// Constant alias for string: `"zenoh/int16"`.
    ///
    /// Usually used for types: `i16`.
    pub const ZENOH_INT16: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 2,
        schema: None,
    });
    /// A VLE-encoded signed little-endian 32bit integer. Binary representation uses two's complement.
    ///
    /// Constant alias for string: `"zenoh/int32"`.
    ///
    /// Usually used for types: `i32`.
    pub const ZENOH_INT32: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 3,
        schema: None,
    });
    /// A VLE-encoded signed little-endian 64bit integer. Binary representation uses two's complement.
    ///
    /// Constant alias for string: `"zenoh/int64"`.
    ///
    /// Usually used for types: `i64`.
    pub const ZENOH_INT64: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 4,
        schema: None,
    });
    /// A VLE-encoded signed little-endian 128bit integer. Binary representation uses two's complement.
    ///
    /// Constant alias for string: `"zenoh/int128"`.
    ///
    /// Usually used for types: `i128`.
    pub const ZENOH_INT128: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 5,
        schema: None,
    });
    /// A VLE-encoded unsigned little-endian 8bit integer.
    ///
    /// Constant alias for string: `"zenoh/uint8"`.
    ///
    /// Usually used for types: `u8`.
    pub const ZENOH_UINT8: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 6,
        schema: None,
    });
    /// A VLE-encoded unsigned little-endian 16bit integer.
    ///
    /// Constant alias for string: `"zenoh/uint16"`.
    ///
    /// Usually used for types: `u16`.
    pub const ZENOH_UINT16: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 7,
        schema: None,
    });
    /// A VLE-encoded unsigned little-endian 32bit integer.
    ///
    /// Constant alias for string: `"zenoh/uint32"`.
    ///
    /// Usually used for types: `u32`.
    pub const ZENOH_UINT32: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 8,
        schema: None,
    });
    /// A VLE-encoded unsigned little-endian 64bit integer.
    ///
    /// Constant alias for string: `"zenoh/uint64"`.
    ///
    /// Usually used for types: `u64`.
    pub const ZENOH_UINT64: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 9,
        schema: None,
    });
    /// A VLE-encoded unsigned little-endian 128bit integer.
    ///
    /// Constant alias for string: `"zenoh/uint128"`.
    ///
    /// Usually used for types: `u128`.
    pub const ZENOH_UINT128: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 10,
        schema: None,
    });
    /// A VLE-encoded 32bit float. Binary representation uses *IEEE 754-2008* *binary32* .
    ///
    /// Constant alias for string: `"zenoh/float32"`.
    ///
    /// Usually used for types: `f32`.
    pub const ZENOH_FLOAT32: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 11,
        schema: None,
    });
    /// A VLE-encoded 64bit float. Binary representation uses *IEEE 754-2008* *binary64*.
    ///
    /// Constant alias for string: `"zenoh/float64"`.
    ///
    /// Usually used for types: `f64`.
    pub const ZENOH_FLOAT64: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 12,
        schema: None,
    });
    /// A boolean. `0` is `false`, `1` is `true`. Other values are invalid.
    ///
    /// Constant alias for string: `"zenoh/bool"`.
    ///
    /// Usually used for types: `bool`.
    pub const ZENOH_BOOL: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 13,
        schema: None,
    });
    /// A UTF-8 string.
    ///
    /// Constant alias for string: `"zenoh/string"`.
    ///
    /// Usually used for types: `String`, `&str`, `Cow<str>`, `char`.
    pub const ZENOH_STRING: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 14,
        schema: None,
    });
    /// A zenoh error.
    ///
    /// Constant alias for string: `"zenoh/error"`.
    ///
    /// Usually used for types: `ReplyError`.
    pub const ZENOH_ERROR: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 15,
        schema: None,
    });

    // - Advanced types may be supported in some of the Zenoh bindings.
    /// An application-specific stream of bytes.
    ///
    /// Constant alias for string: `"application/octet-stream"`.
    pub const APPLICATION_OCTET_STREAM: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 16,
        schema: None,
    });
    /// A textual file.
    ///
    /// Constant alias for string: `"text/plain"`.
    pub const TEXT_PLAIN: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 17,
        schema: None,
    });
    /// JSON data intended to be consumed by an application.
    ///
    /// Constant alias for string: `"application/json"`.
    pub const APPLICATION_JSON: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 18,
        schema: None,
    });
    /// JSON data intended to be human readable.
    ///
    /// Constant alias for string: `"text/json"`.
    pub const TEXT_JSON: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 19,
        schema: None,
    });
    /// A Common Data Representation (CDR)-encoded data.
    ///
    /// Constant alias for string: `"application/cdr"`.
    pub const APPLICATION_CDR: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 20,
        schema: None,
    });
    /// A Concise Binary Object Representation (CBOR)-encoded data.
    ///
    /// Constant alias for string: `"application/cbor"`.
    pub const APPLICATION_CBOR: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 21,
        schema: None,
    });
    /// YAML data intended to be consumed by an application.
    ///
    /// Constant alias for string: `"application/yaml"`.
    pub const APPLICATION_YAML: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 22,
        schema: None,
    });
    /// YAML data intended to be human readable.
    ///
    /// Constant alias for string: `"text/yaml"`.
    pub const TEXT_YAML: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 23,
        schema: None,
    });
    /// JSON5 encoded data that are human readable.
    ///
    /// Constant alias for string: `"text/json5"`.
    pub const TEXT_JSON5: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 24,
        schema: None,
    });
    /// A Python object serialized using [pickle](https://docs.python.org/3/library/pickle.html).
    ///
    /// Constant alias for string: `"application/python-serialized-object"`.
    pub const APPLICATION_PYTHON_SERIALIZED_OBJECT: Encoding =
        Self(zenoh_protocol::core::Encoding {
            id: 25,
            schema: None,
        });
    /// An application-specific protobuf-encoded data.
    ///
    /// Constant alias for string: `"application/protobuf"`.
    pub const APPLICATION_PROTOBUF: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 26,
        schema: None,
    });
    /// A Java serialized object.
    ///
    /// Constant alias for string: `"application/java-serialized-object"`.
    pub const APPLICATION_JAVA_SERIALIZED_OBJECT: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 27,
        schema: None,
    });
    /// An [openmetrics](https://github.com/OpenObservability/OpenMetrics) data, common used by [Prometheus](https://prometheus.io/).
    ///
    /// Constant alias for string: `"application/openmetrics-text"`.
    pub const APPLICATION_OPENMETRICS_TEXT: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 28,
        schema: None,
    });
    /// A Portable Network Graphics (PNG) image.
    ///
    /// Constant alias for string: `"image/png"`.
    pub const IMAGE_PNG: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 29,
        schema: None,
    });
    /// A Joint Photographic Experts Group (JPEG) image.
    ///
    /// Constant alias for string: `"image/jpeg"`.
    pub const IMAGE_JPEG: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 30,
        schema: None,
    });
    /// A Graphics Interchange Format (GIF) image.
    ///
    /// Constant alias for string: `"image/gif"`.
    pub const IMAGE_GIF: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 31,
        schema: None,
    });
    /// A BitMap (BMP) image.
    ///
    /// Constant alias for string: `"image/bmp"`.
    pub const IMAGE_BMP: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 32,
        schema: None,
    });
    /// A Web Portable (WebP) image.
    ///
    ///  Constant alias for string: `"image/webp"`.
    pub const IMAGE_WEBP: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 33,
        schema: None,
    });
    /// An XML file intended to be consumed by an application..
    ///
    /// Constant alias for string: `"application/xml"`.
    pub const APPLICATION_XML: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 34,
        schema: None,
    });
    /// An encoded a list of tuples, each consisting of a name and a value.
    ///
    /// Constant alias for string: `"application/x-www-form-urlencoded"`.
    pub const APPLICATION_X_WWW_FORM_URLENCODED: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 35,
        schema: None,
    });
    /// An HTML file.
    ///
    /// Constant alias for string: `"text/html"`.
    pub const TEXT_HTML: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 36,
        schema: None,
    });
    /// An XML file that is human readable.
    ///
    /// Constant alias for string: `"text/xml"`.
    pub const TEXT_XML: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 37,
        schema: None,
    });
    /// A CSS file.
    ///
    /// Constant alias for string: `"text/css"`.
    pub const TEXT_CSS: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 38,
        schema: None,
    });
    /// A JavaScript file.
    ///
    /// Constant alias for string: `"text/javascript"`.
    pub const TEXT_JAVASCRIPT: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 39,
        schema: None,
    });
    /// A MarkDown file.
    ///
    /// Constant alias for string: `"text/markdown"`.
    pub const TEXT_MARKDOWN: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 40,
        schema: None,
    });
    /// A CSV file.
    ///
    /// Constant alias for string: `"text/csv"`.
    pub const TEXT_CSV: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 41,
        schema: None,
    });
    /// An application-specific SQL query.
    ///
    /// Constant alias for string: `"application/sql"`.
    pub const APPLICATION_SQL: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 42,
        schema: None,
    });
    /// Constrained Application Protocol (CoAP) data intended for CoAP-to-HTTP and HTTP-to-CoAP proxies.
    ///
    /// Constant alias for string: `"application/coap-payload"`.
    pub const APPLICATION_COAP_PAYLOAD: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 43,
        schema: None,
    });
    /// Defines a JSON document structure for expressing a sequence of operations to apply to a JSON document.
    ///
    /// Constant alias for string: `"application/json-patch+json"`.
    pub const APPLICATION_JSON_PATCH_JSON: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 44,
        schema: None,
    });
    /// A JSON text sequence consists of any number of JSON texts, all encoded in UTF-8.
    ///
    /// Constant alias for string: `"application/json-seq"`.
    pub const APPLICATION_JSON_SEQ: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 45,
        schema: None,
    });
    /// A JSONPath defines a string syntax for selecting and extracting JSON values from within a given JSON value.
    ///
    /// Constant alias for string: `"application/jsonpath"`.
    pub const APPLICATION_JSONPATH: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 46,
        schema: None,
    });
    /// A JSON Web Token (JWT).
    ///
    /// Constant alias for string: `"application/jwt"`.
    pub const APPLICATION_JWT: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 47,
        schema: None,
    });
    /// An application-specific MPEG-4 encoded data, either audio or video.
    ///
    /// Constant alias for string: `"application/mp4"`.
    pub const APPLICATION_MP4: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 48,
        schema: None,
    });
    /// A SOAP 1.2 message serialized as XML 1.0.
    ///
    /// Constant alias for string: `"application/soap+xml"`.
    pub const APPLICATION_SOAP_XML: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 49,
        schema: None,
    });
    /// A YANG-encoded data commonly used by the Network Configuration Protocol (NETCONF).
    ///
    /// Constant alias for string: `"application/yang"`.
    pub const APPLICATION_YANG: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 50,
        schema: None,
    });
    /// A MPEG-4 Advanced Audio Coding (AAC) media.
    ///
    /// Constant alias for string: `"audio/aac"`.
    pub const AUDIO_AAC: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 51,
        schema: None,
    });
    /// A Free Lossless Audio Codec (FLAC) media.
    ///
    /// Constant alias for string: `"audio/flac"`.
    pub const AUDIO_FLAC: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 52,
        schema: None,
    });
    /// An audio codec defined in MPEG-1, MPEG-2, MPEG-4, or registered at the MP4 registration authority.
    ///
    /// Constant alias for string: `"audio/mp4"`.
    pub const AUDIO_MP4: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 53,
        schema: None,
    });
    /// An Ogg-encapsulated audio stream.
    ///
    /// Constant alias for string: `"audio/ogg"`.
    pub const AUDIO_OGG: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 54,
        schema: None,
    });
    /// A Vorbis-encoded audio stream.
    ///
    /// Constant alias for string: `"audio/vorbis"`.
    pub const AUDIO_VORBIS: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 55,
        schema: None,
    });
    /// A h261-encoded video stream.
    ///
    /// Constant alias for string: `"video/h261"`.
    pub const VIDEO_H261: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 56,
        schema: None,
    });
    /// A h263-encoded video stream.
    ///
    /// Constant alias for string: `"video/h263"`.
    pub const VIDEO_H263: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 57,
        schema: None,
    });
    /// A h264-encoded video stream.
    ///
    /// Constant alias for string: `"video/h264"`.
    pub const VIDEO_H264: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 58,
        schema: None,
    });
    /// A h265-encoded video stream.
    ///
    /// Constant alias for string: `"video/h265"`.
    pub const VIDEO_H265: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 59,
        schema: None,
    });
    /// A h266-encoded video stream.
    ///
    /// Constant alias for string: `"video/h266"`.
    pub const VIDEO_H266: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 60,
        schema: None,
    });
    /// A video codec defined in MPEG-1, MPEG-2, MPEG-4, or registered at the MP4 registration authority.
    ///
    /// Constant alias for string: `"video/mp4"`.
    pub const VIDEO_MP4: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 61,
        schema: None,
    });
    /// An Ogg-encapsulated video stream.
    ///
    /// Constant alias for string: `"video/ogg"`.
    pub const VIDEO_OGG: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 62,
        schema: None,
    });
    /// An uncompressed, studio-quality video stream.
    ///
    /// Constant alias for string: `"video/raw"`.
    pub const VIDEO_RAW: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 63,
        schema: None,
    });
    /// A VP8-encoded video stream.
    ///
    /// Constant alias for string: `"video/vp8"`.
    pub const VIDEO_VP8: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 64,
        schema: None,
    });
    /// A VP9-encoded video stream.
    ///
    /// Constant alias for string: `"video/vp9"`.
    pub const VIDEO_VP9: Encoding = Self(zenoh_protocol::core::Encoding {
        id: 65,
        schema: None,
    });

    const ID_TO_STR: phf::Map<EncodingId, &'static str> = phf_map! {
        0u16 => "zenoh/bytes",
        1u16 => "zenoh/int8",
        2u16 => "zenoh/int16",
        3u16 => "zenoh/int32",
        4u16 => "zenoh/int64",
        5u16 => "zenoh/int128",
        6u16 => "zenoh/uint8",
        7u16 => "zenoh/uint16",
        8u16 => "zenoh/uint32",
        9u16 => "zenoh/uint64",
        10u16 => "zenoh/uint128",
        11u16 => "zenoh/float32",
        12u16 => "zenoh/float64",
        13u16 => "zenoh/bool",
        14u16 => "zenoh/string",
        15u16 => "zenoh/error",
        16u16 => "application/octet-stream",
        17u16 => "text/plain",
        18u16 => "application/json",
        19u16 => "text/json",
        20u16 => "application/cdr",
        21u16 => "application/cbor",
        22u16 => "application/yaml",
        23u16 => "text/yaml",
        24u16 => "text/json5",
        25u16 => "application/python-serialized-object",
        26u16 => "application/protobuf",
        27u16 => "application/java-serialized-object",
        28u16 => "application/openmetrics-text",
        29u16 => "image/png",
        30u16 => "image/jpeg",
        31u16 => "image/gif",
        32u16 => "image/bmp",
        33u16 => "image/webp",
        34u16 => "application/xml",
        35u16 => "application/x-www-form-urlencoded",
        36u16 => "text/html",
        37u16 => "text/xml",
        38u16 => "text/css",
        39u16 => "text/javascript",
        40u16 => "text/markdown",
        41u16 => "text/csv",
        42u16 => "application/sql",
        43u16 => "application/coap-payload",
        44u16 => "application/json-patch+json",
        45u16 => "application/json-seq",
        46u16 => "application/jsonpath",
        47u16 => "application/jwt",
        48u16 => "application/mp4",
        49u16 => "application/soap+xml",
        50u16 => "application/yang",
        51u16 => "audio/aac",
        52u16 => "audio/flac",
        53u16 => "audio/mp4",
        54u16 => "audio/ogg",
        55u16 => "audio/vorbis",
        56u16 => "video/h261",
        57u16 => "video/h263",
        58u16 => "video/h264",
        59u16 => "video/h265",
        60u16 => "video/h266",
        61u16 => "video/mp4",
        62u16 => "video/ogg",
        63u16 => "video/raw",
        64u16 => "video/vp8",
        65u16 => "video/vp9",
    };

    const STR_TO_ID: phf::Map<&'static str, EncodingId> = phf_map! {
        "zenoh/bytes" => 0u16,
        "zenoh/int8" => 1u16,
        "zenoh/int16" => 2u16,
        "zenoh/int32" => 3u16,
        "zenoh/int64" => 4u16,
        "zenoh/int128" => 5u16,
        "zenoh/uint8" => 6u16,
        "zenoh/uint16" => 7u16,
        "zenoh/uint32" => 8u16,
        "zenoh/uint64" => 9u16,
        "zenoh/uint128" => 10u16,
        "zenoh/float32" => 11u16,
        "zenoh/float64" => 12u16,
        "zenoh/bool" => 13u16,
        "zenoh/string" => 14u16,
        "zenoh/error" => 15u16,
        "application/octet-stream" => 16u16,
        "text/plain" => 17u16,
        "application/json" => 18u16,
        "text/json" => 19u16,
        "application/cdr" => 20u16,
        "application/cbor" => 21u16,
        "application/yaml" => 22u16,
        "text/yaml" => 23u16,
        "text/json5" => 24u16,
        "application/python-serialized-object" => 25u16,
        "application/protobuf" => 26u16,
        "application/java-serialized-object" => 27u16,
        "application/openmetrics-text" => 28u16,
        "image/png" => 29u16,
        "image/jpeg" => 30u16,
        "image/gif" => 31u16,
        "image/bmp" => 32u16,
        "image/webp" => 33u16,
        "application/xml" => 34u16,
        "application/x-www-form-urlencoded" => 35u16,
        "text/html" => 36u16,
        "text/xml" => 37u16,
        "text/css" => 38u16,
        "text/javascript" => 39u16,
        "text/markdown" => 40u16,
        "text/csv" => 41u16,
        "application/sql" => 42u16,
        "application/coap-payload" => 43u16,
        "application/json-patch+json" => 44u16,
        "application/json-seq" => 45u16,
        "application/jsonpath" => 46u16,
        "application/jwt" => 47u16,
        "application/mp4" => 48u16,
        "application/soap+xml" => 49u16,
        "application/yang" => 50u16,
        "audio/aac" => 51u16,
        "audio/flac" => 52u16,
        "audio/mp4" => 53u16,
        "audio/ogg" => 54u16,
        "audio/vorbis" => 55u16,
        "video/h261" => 56u16,
        "video/h263" => 57u16,
        "video/h264" => 58u16,
        "video/h265" => 59u16,
        "video/h266" => 60u16,
        "video/mp4" => 61u16,
        "video/ogg" => 62u16,
        "video/raw" => 63u16,
        "video/vp8" => 64u16,
        "video/vp9" => 65u16,
    };

    /// The default [`Encoding`] is [`ZENOH_BYTES`](Encoding::ZENOH_BYTES).
    pub const fn default() -> Self {
        Self::ZENOH_BYTES
    }

    /// Set a schema to this encoding. Zenoh does not define what a schema is and its semantichs is left to the implementer.
    /// E.g. a common schema for `text/plain` encoding is `utf-8`.
    pub fn with_schema<S>(mut self, s: S) -> Self
    where
        S: Into<String>,
    {
        let s: String = s.into();
        self.0.schema = Some(s.into_boxed_str().into_boxed_bytes().into());
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
        } else {
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

#[allow(dead_code)]
// - Encoding trait
pub trait EncodingMapping {
    const ENCODING: Encoding;
}

// Bytes
impl EncodingMapping for ZBytes {
    const ENCODING: Encoding = Encoding::ZENOH_BYTES;
}

impl EncodingMapping for ZBuf {
    const ENCODING: Encoding = Encoding::ZENOH_BYTES;
}

impl EncodingMapping for Vec<u8> {
    const ENCODING: Encoding = Encoding::ZENOH_BYTES;
}

impl EncodingMapping for &[u8] {
    const ENCODING: Encoding = Encoding::ZENOH_BYTES;
}

impl EncodingMapping for Cow<'_, [u8]> {
    const ENCODING: Encoding = Encoding::ZENOH_BYTES;
}

// String
impl EncodingMapping for String {
    const ENCODING: Encoding = Encoding::ZENOH_STRING;
}

impl EncodingMapping for &str {
    const ENCODING: Encoding = Encoding::ZENOH_STRING;
}

impl EncodingMapping for Cow<'_, str> {
    const ENCODING: Encoding = Encoding::ZENOH_STRING;
}

// Zenoh unsigned integers
impl EncodingMapping for u8 {
    const ENCODING: Encoding = Encoding::ZENOH_UINT8;
}

impl EncodingMapping for u16 {
    const ENCODING: Encoding = Encoding::ZENOH_UINT16;
}

impl EncodingMapping for u32 {
    const ENCODING: Encoding = Encoding::ZENOH_UINT32;
}

impl EncodingMapping for u64 {
    const ENCODING: Encoding = Encoding::ZENOH_UINT64;
}

impl EncodingMapping for u128 {
    const ENCODING: Encoding = Encoding::ZENOH_UINT128;
}

impl EncodingMapping for usize {
    #[cfg(target_pointer_width = "8")]
    const ENCODING: Encoding = Encoding::ZENOH_UINT8;
    #[cfg(target_pointer_width = "16")]
    const ENCODING: Encoding = Encoding::ZENOH_UINT16;
    #[cfg(target_pointer_width = "32")]
    const ENCODING: Encoding = Encoding::ZENOH_UINT32;
    #[cfg(target_pointer_width = "64")]
    const ENCODING: Encoding = Encoding::ZENOH_UINT64;
    #[cfg(target_pointer_width = "128")]
    const ENCODING: Encoding = Encoding::ZENOH_UINT128;
}

// Zenoh signed integers
impl EncodingMapping for i8 {
    const ENCODING: Encoding = Encoding::ZENOH_INT8;
}

impl EncodingMapping for i16 {
    const ENCODING: Encoding = Encoding::ZENOH_INT16;
}

impl EncodingMapping for i32 {
    const ENCODING: Encoding = Encoding::ZENOH_INT32;
}

impl EncodingMapping for i64 {
    const ENCODING: Encoding = Encoding::ZENOH_INT64;
}

impl EncodingMapping for i128 {
    const ENCODING: Encoding = Encoding::ZENOH_INT128;
}

impl EncodingMapping for isize {
    #[cfg(target_pointer_width = "8")]
    const ENCODING: Encoding = Encoding::ZENOH_INT8;
    #[cfg(target_pointer_width = "16")]
    const ENCODING: Encoding = Encoding::ZENOH_INT16;
    #[cfg(target_pointer_width = "32")]
    const ENCODING: Encoding = Encoding::ZENOH_INT32;
    #[cfg(target_pointer_width = "64")]
    const ENCODING: Encoding = Encoding::ZENOH_INT64;
    #[cfg(target_pointer_width = "128")]
    const ENCODING: Encoding = Encoding::ZENOH_INT128;
}

// Zenoh floats
impl EncodingMapping for f32 {
    const ENCODING: Encoding = Encoding::ZENOH_FLOAT32;
}

impl EncodingMapping for f64 {
    const ENCODING: Encoding = Encoding::ZENOH_FLOAT64;
}

// Zenoh bool
impl EncodingMapping for bool {
    const ENCODING: Encoding = Encoding::ZENOH_BOOL;
}

// - Zenoh advanced types encoders/decoders
impl EncodingMapping for serde_json::Value {
    const ENCODING: Encoding = Encoding::APPLICATION_JSON;
}

impl EncodingMapping for serde_yaml::Value {
    const ENCODING: Encoding = Encoding::APPLICATION_YAML;
}

impl EncodingMapping for serde_cbor::Value {
    const ENCODING: Encoding = Encoding::APPLICATION_CBOR;
}

impl EncodingMapping for serde_pickle::Value {
    const ENCODING: Encoding = Encoding::APPLICATION_PYTHON_SERIALIZED_OBJECT;
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

// - Zenoh SHM
#[cfg(feature = "shared-memory")]
impl EncodingMapping for ZShm {
    const ENCODING: Encoding = Encoding::ZENOH_BYTES;
}
#[cfg(feature = "shared-memory")]
impl EncodingMapping for ZShmMut {
    const ENCODING: Encoding = Encoding::ZENOH_BYTES;
}
