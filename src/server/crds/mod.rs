/// CRD definitions owned and managed by Rise.
///
/// Use `rise backend crds` to output these as YAML for committing into the Helm chart.
pub mod snowflake_postgres;

use anyhow::Result;

/// Return the serialised YAML for all CRDs managed by Rise.
///
/// The output is a multi-document YAML stream (documents separated by `---`).
/// It is intended to be committed into `helm/rise/crds/` so that Helm installs
/// the CRDs before any other chart resources.
pub fn get_all_crds_yaml() -> Result<String> {
    use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
    use kube::core::CustomResourceExt;

    let crds: Vec<CustomResourceDefinition> = vec![snowflake_postgres::SnowflakePostgres::crd()];

    let mut parts: Vec<String> = Vec::new();
    for crd in &crds {
        let yaml = serde_yaml::to_string(crd)?;
        parts.push(yaml);
    }

    Ok(parts.join("---\n"))
}
