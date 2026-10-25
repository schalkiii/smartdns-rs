use std::path::{Path, PathBuf};

use rcgen::{BasicConstraints, CertificateParams, DnType, IsCa, Issuer, KeyPair};

use crate::dns_conf::RuntimeConfig;

/// 生成（或复用已生成的）自签证书，返回 `(cert_path, key_path)`。
///
/// 证书放在 `data-dir`（未配置时用临时目录），文件名固定，进程重启后复用；
/// 配置 `bind-cert-san` / `bind-cert-validity-days` 会体现在生成结果里；
/// 配置 `bind-cert-root-key-file` 时用该密钥作为 CA 签发，否则自签。
pub fn generate(cfg: &RuntimeConfig) -> Result<(PathBuf, PathBuf), crate::Error> {
    let dir = cfg
        .data_dir()
        .map(Path::to_path_buf)
        .unwrap_or_else(std::env::temp_dir);
    let cert_path = dir.join("smartdns-selfsigned.crt");
    let key_path = dir.join("smartdns-selfsigned.key");

    if cert_path.is_file() && key_path.is_file() {
        return Ok((cert_path, key_path));
    }

    std::fs::create_dir_all(&dir)?;

    let mut sans: Vec<String> = vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
    ];
    sans.extend(cfg.bind_cert_san().iter().cloned());

    let mut params = CertificateParams::new(sans)
        .map_err(|err| crate::Error::LoadCertificateFailed(cert_path.clone(), err.to_string()))?;
    params
        .distinguished_name
        .push(DnType::CommonName, "SmartDNS");

    if let Some(days) = cfg.bind_cert_validity_days() {
        let now = time::OffsetDateTime::now_utc();
        params.not_before = now;
        params.not_after = now + time::Duration::days(days as i64);
    }

    let key_pair = KeyPair::generate()
        .map_err(|err| crate::Error::LoadCertificateKeyFailed(key_path.clone(), err.to_string()))?;

    let cert = match cfg.bind_cert_root_key_file() {
        Some(root_key_path) => {
            let pem = std::fs::read_to_string(root_key_path)?;
            let ca_key = KeyPair::from_pem(&pem).map_err(|err| {
                crate::Error::LoadCertificateKeyFailed(root_key_path.to_path_buf(), err.to_string())
            })?;

            let mut ca_params = CertificateParams::new(Vec::<String>::new()).map_err(|err| {
                crate::Error::LoadCertificateFailed(cert_path.clone(), err.to_string())
            })?;
            ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
            ca_params
                .distinguished_name
                .push(DnType::CommonName, "SmartDNS Root CA");

            let issuer = Issuer::from_params(&ca_params, &ca_key);
            params.signed_by(&key_pair, &issuer).map_err(|err| {
                crate::Error::LoadCertificateFailed(cert_path.clone(), err.to_string())
            })?
        }
        None => params.self_signed(&key_pair).map_err(|err| {
            crate::Error::LoadCertificateFailed(cert_path.clone(), err.to_string())
        })?,
    };

    std::fs::write(&cert_path, cert.pem())?;
    std::fs::write(&key_path, key_pair.serialize_pem())?;

    Ok((cert_path, key_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_self_signed() {
        let dir = std::env::temp_dir().join("smartdns-test-selfsigned");
        let _ = std::fs::remove_dir_all(&dir);

        let data_dir = format!("data-dir {}", dir.display());
        let cfg = RuntimeConfig::builder()
            .with(&data_dir)
            .with("bind-cert-san dns.example.com")
            .with("bind-cert-validity-days 30")
            .build()
            .unwrap();

        let (cert, key) = generate(&cfg).expect("self-signed cert should be generated");
        assert!(cert.is_file());
        assert!(key.is_file());

        let cert_pem = std::fs::read_to_string(&cert).unwrap();
        assert!(cert_pem.contains("BEGIN CERTIFICATE"));
        let key_pem = std::fs::read_to_string(&key).unwrap();
        assert!(key_pem.contains("PRIVATE KEY"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
