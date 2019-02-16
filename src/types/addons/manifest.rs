use serde_derive::*;

use crate::types::addons::ResourceRef;
// https://serde.rs/string-or-struct.html
use semver::Version;
use serde::de::{self, Deserialize, Deserializer, MapAccess, Visitor};
use std::fmt;
use std::marker::PhantomData;
use std::str::FromStr;

// Resource descriptors
// those define how a resource may be requested
#[derive(Debug, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestResource {
    pub name: String,
    pub types: Option<Vec<String>>,
    pub id_prefixes: Option<Vec<String>>,
}
impl FromStr for ManifestResource {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(ManifestResource {
            name: s.to_string(),
            types: None,
            id_prefixes: None,
        })
    }
}

// Extra descriptors
// those define the extra properties that may be passed for a catalog
#[derive(Debug, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestExtraProp {
    name: String,
    #[serde(default)]
    is_required: bool,
    values: Option<Vec<String>>,
}
#[derive(Debug, PartialEq, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ManifestExtra {
    Full {
        #[serde(rename = "extra")]
        props: Vec<ManifestExtraProp>,
    },
    // Simple notation, which was the standard before v3.1 protocol: https://github.com/Stremio/stremio-addon-sdk/milestone/1
    // kind of obsolete, but addons may use it
    Simple {
        #[serde(default, rename = "extraRequired")]
        required: Vec<String>,
        #[serde(default, rename = "extraSupported")]
        supported: Vec<String>,
    },
}

#[derive(Debug, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestCatalog {
    #[serde(rename = "type")]
    pub type_name: String,
    pub id: String,
    pub name: Option<String>,
    #[serde(flatten)]
    pub extra: ManifestExtra,
}
impl ManifestCatalog {
    pub fn is_extra_supported(&self, extra: &[(String, String)]) -> bool {
        match &self.extra {
            ManifestExtra::Full { props } => {
                let all_supported = extra
                    .iter()
                    .all(|(k, _)| props.iter().any(|e| k == &e.name));
                let requirements_satisfied = props
                    .iter()
                    .filter(|e| e.is_required)
                    .all(|e| extra.iter().any(|(k, _)| k == &e.name));
                all_supported && requirements_satisfied
            }
            ManifestExtra::Simple {
                ref required,
                ref supported,
            } => {
                let all_supported = extra.iter().all(|(k, _)| supported.contains(k));
                let requirements_satisfied =
                    required.iter().all(|kr| extra.iter().any(|(k, _)| kr == k));
                all_supported && requirements_satisfied
            }
        }
    }
}

// The manifest itself
// the tricky thing here is that resources may either be provided in a short notation, e.g. `"stream"`
// or long e.g. `{ name: "stream", types: ["movie"], id_prefixes: ["tt"] }`
#[derive(Debug, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub id: String,
    pub version: Version,
    pub name: String,
    pub description: Option<String>,
    pub logo: Option<String>,
    pub background: Option<String>,
    // @TODO catalogs
    #[serde(deserialize_with = "vec_manifest_resource")]
    pub resources: Vec<ManifestResource>,
    pub types: Option<Vec<String>>,
    pub id_prefixes: Option<Vec<String>>,
    // @TODO: more efficient data structure?
    //pub behavior_hints: Vec<String>,
    #[serde(default)]
    pub catalogs: Vec<ManifestCatalog>,
}

impl Manifest {
    // @TODO: test
    // assert_eq!(cinemeta_m.is_supported("meta", "movie", "tt0234"), true);
    // assert_eq!(cinemeta_m.is_supported("meta", "movie", "somethingElse"), false));
    // assert_eq!(cinemeta_m.is_supported("stream", "movie", "tt0234"), false);
    pub fn is_supported(
        &self,
        ResourceRef {
            resource,
            type_name,
            id,
            extra,
        }: &ResourceRef,
    ) -> bool {
        // catalogs are a special case
        if resource == "catalog" {
            return self
                .catalogs
                .iter()
                .any(|c| &c.type_name == type_name && &c.id == id && c.is_extra_supported(&extra));
        }
        let res = match self.resources.iter().find(|res| &res.name == resource) {
            None => return false,
            Some(resource) => resource,
        };
        // types MUST contain type_name
        // and if there is id_prefixes, our id should start with at least one of them
        let is_types_match = res
            .types
            .as_ref()
            .or_else(|| self.types.as_ref())
            .map_or(false, |types| types.iter().any(|t| t == type_name));
        let is_id_match = res
            .id_prefixes
            .as_ref()
            .or_else(|| self.id_prefixes.as_ref())
            .map_or(true, |prefixes| {
                prefixes.iter().any(|pref| id.starts_with(pref))
            });
        is_types_match && is_id_match
    }
}

// @TODO: this also needs to be a crate, kind of: https://github.com/serde-rs/serde/issues/723
fn vec_manifest_resource<'de, D>(deserializer: D) -> Result<Vec<ManifestResource>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    struct Wrapper(#[serde(deserialize_with = "string_or_struct")] ManifestResource);

    let v = Vec::deserialize(deserializer)?;
    Ok(v.into_iter().map(|Wrapper(a)| a).collect())
}
// @TODO: move string_or_struct to a crate
fn string_or_struct<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: Deserialize<'de> + FromStr<Err = ()>,
    D: Deserializer<'de>,
{
    struct StringOrStruct<T>(PhantomData<fn() -> T>);

    impl<'de, T> Visitor<'de> for StringOrStruct<T>
    where
        T: Deserialize<'de> + FromStr<Err = ()>,
    {
        type Value = T;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("string or map")
        }

        fn visit_str<E>(self, value: &str) -> Result<T, E>
        where
            E: de::Error,
        {
            Ok(FromStr::from_str(value).unwrap())
        }

        fn visit_map<M>(self, visitor: M) -> Result<T, M::Error>
        where
            M: MapAccess<'de>,
        {
            Deserialize::deserialize(de::value::MapAccessDeserializer::new(visitor))
        }
    }

    deserializer.deserialize_any(StringOrStruct(PhantomData))
}
