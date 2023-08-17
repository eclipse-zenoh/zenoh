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

use std::{collections::HashSet, io::Read, path::Path};

use serde::Deserialize;
use serde_json::{Map, Value};
use zenoh_result::{bail, zerror, ZResult};

pub(crate) fn deserialize_from_file<T, P>(path: P) -> ZResult<T>
where
    for<'de> T: Deserialize<'de>,
    P: AsRef<Path>,
{
    match std::fs::File::open(path.as_ref()) {
        Ok(mut f) => {
            let mut content = String::new();
            if let Err(e) = f.read_to_string(&mut content) {
                bail!(e)
            }
            match path.as_ref()
                .extension()
                .map(|s| s.to_str().unwrap())
            {
                Some("json") | Some("json5") => match json5::Deserializer::from_str(&content) {
                    Ok(mut d) => T::deserialize(&mut d).map_err(|e| zerror!("JSON5 error: {}", e).into()),
                    Err(e) => Err(zerror!("JSON5 error: {}", e).into()),
                },
                Some("yaml") => {
                    let d = serde_yaml::Deserializer::from_str(&content);
                    T::deserialize(d).map_err(|e| zerror!("YAML error: {}", e).into())
                },
                Some(other) => bail!("Unsupported file type '.{}' (.json, .json5 and .yaml are supported)", other),
                None => bail!("Unsupported file type. File must have an extension (.json, .json5 and .yaml supported)")
            }
        }
        Err(e) => bail!(e),
    }
}

pub(crate) fn recursive_include(
    title: &str, // path in format "filename::object -> filename::object -> ..." for error reporting
    values: &mut Map<String, Value>,
    loop_detector: &mut HashSet<String>,
    include_property_name: &str,
) -> ZResult<()> {
    if !loop_detector.insert(title.to_string()) {
        bail!("{} : include loop detected", title,);
    }
    let Some(include_path) = values.get(include_property_name) else { return Ok(()) };
    let Some(include_path)= include_path.as_str() else {
        bail!("{} : '{}' property must have string type", title, include_property_name);
    };
    let mut include_values: Value = deserialize_from_file(include_path)?;
    let Some(include_values) = include_values.as_object_mut() else {
        bail!("{} : '{}' property: included file '{}' must be an object", title, include_property_name, include_path);
    };
    for (k, v) in include_values.iter_mut() {
        if let Some(values) = v.as_object_mut() {
            let title = format!("{} -> {}::{}", title, include_path, k);
            recursive_include(title.as_str(), values, loop_detector, include_property_name)?;
        }
    }
    values.append(include_values);
    Ok(())
}
