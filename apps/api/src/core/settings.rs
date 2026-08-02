#[derive(Clone, Debug)]
pub struct Settings {
    pub database_url: String,
    pub jwt_secret: String,
    pub admin_password: String,
    pub server_host: String,
    pub server_port: u16,
    pub db_max_connections: u32,
    pub docker_network: String,
    pub container_runtime: String,
    pub host_gateway_ip: String,
    pub host_port_start: u16,
    pub host_port_end: u16,
}

impl Settings {
    pub fn new() -> Result<Self, String> {
        Self::from_env(std::env::vars())
    }

    pub fn from_env<I, K, V>(vars: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let env: std::collections::HashMap<String, String> = vars
            .into_iter()
            .map(|(k, v)| (k.as_ref().to_string(), v.as_ref().to_string()))
            .collect();

        let get = |key: &str| -> Option<String> {
            env.get(key).cloned()
        };

        Ok(Self {
            database_url: get("DATABASE_URL")
                .ok_or_else(|| "DATABASE_URL must be set".to_string())?,
            jwt_secret: get("JWT_SECRET")
                .ok_or_else(|| "JWT_SECRET must be set".to_string())?,
            admin_password: get("ADMIN_PASSWORD")
                .unwrap_or_else(|| "admin".to_string()),
            server_host: get("SERVER_HOST")
                .unwrap_or_else(|| "0.0.0.0".to_string()),
            server_port: get("SERVER_PORT")
                .unwrap_or_else(|| "3000".to_string())
                .parse()
                .map_err(|e| format!("SERVER_PORT invalid: {}", e))?,
            db_max_connections: get("DB_MAX_CONNECTIONS")
                .unwrap_or_else(|| "5".to_string())
                .parse()
                .map_err(|e| format!("DB_MAX_CONNECTIONS invalid: {}", e))?,
            docker_network: get("DOCKER_NETWORK")
                .unwrap_or_else(|| "ow-network".to_string()),
            container_runtime: get("OW_CONTAINER_RUNTIME")
                .unwrap_or_else(|| "docker".to_string()),
            host_gateway_ip: get("OW_HOST_GATEWAY_IP")
                .unwrap_or_else(|| "172.17.0.1".to_string()),
            host_port_start: get("OW_HOST_PORT_START")
                .unwrap_or_else(|| "10000".to_string())
                .parse()
                .map_err(|e| format!("OW_HOST_PORT_START invalid: {}", e))?,
            host_port_end: get("OW_HOST_PORT_END")
                .unwrap_or_else(|| "20000".to_string())
                .parse()
                .map_err(|e| format!("OW_HOST_PORT_END invalid: {}", e))?,
        })
    }

    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.server_host, self.server_port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn test_settings_loads_from_dotenv() {
        let settings = Settings {
            database_url: "postgres://localhost/test".to_string(),
            jwt_secret: "test".to_string(),
            admin_password: "admin".to_string(),
            server_host: "0.0.0.0".to_string(),
            server_port: 3000,
            db_max_connections: 5,
            docker_network: "ow-network".to_string(),
            container_runtime: "docker".to_string(),
            host_gateway_ip: "172.17.0.1".to_string(),
            host_port_start: 10000,
            host_port_end: 20000,
        };
        assert_eq!(settings.bind_address(), "0.0.0.0:3000");
    }

    #[test]
    fn test_bind_address_custom() {
        let settings = Settings {
            database_url: String::new(),
            jwt_secret: String::new(),
            admin_password: String::new(),
            server_host: "127.0.0.1".to_string(),
            server_port: 8080,
            db_max_connections: 10,
            docker_network: "ow-network".to_string(),
            container_runtime: "docker".to_string(),
            host_gateway_ip: "172.17.0.1".to_string(),
            host_port_start: 10000,
            host_port_end: 20000,
        };
        assert_eq!(settings.bind_address(), "127.0.0.1:8080");
    }

    #[test]
    fn test_settings_debug() {
        let settings = Settings {
            database_url: "postgres://localhost/test".to_string(),
            jwt_secret: "secret".to_string(),
            admin_password: "pass".to_string(),
            server_host: "0.0.0.0".to_string(),
            server_port: 3000,
            db_max_connections: 5,
            docker_network: "ow-network".to_string(),
            container_runtime: "docker".to_string(),
            host_gateway_ip: "172.17.0.1".to_string(),
            host_port_start: 10000,
            host_port_end: 20000,
        };
        let debug = format!("{:?}", settings);
        assert!(debug.contains("Settings"));
        assert!(debug.contains("0.0.0.0"));
    }

    #[test]
    fn test_settings_clone() {
        let settings = Settings {
            database_url: "postgres://localhost/test".to_string(),
            jwt_secret: "secret".to_string(),
            admin_password: "pass".to_string(),
            server_host: "0.0.0.0".to_string(),
            server_port: 3000,
            db_max_connections: 5,
            docker_network: "ow-network".to_string(),
            container_runtime: "docker".to_string(),
            host_gateway_ip: "172.17.0.1".to_string(),
            host_port_start: 10000,
            host_port_end: 20000,
        };
        let cloned = settings.clone();
        assert_eq!(settings.database_url, cloned.database_url);
        assert_eq!(settings.jwt_secret, cloned.jwt_secret);
        assert_eq!(settings.db_max_connections, cloned.db_max_connections);
    }

    #[test]
    fn test_settings_new_missing_database_url() {
        let result = Settings::from_env(vars(&[("JWT_SECRET", "test")]));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("DATABASE_URL"));
    }

    #[test]
    fn test_settings_new_missing_jwt_secret() {
        let result = Settings::from_env(vars(&[("DATABASE_URL", "postgres://localhost/test")]));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("JWT_SECRET"));
    }

    #[test]
    fn test_settings_new_invalid_server_port() {
        let result = Settings::from_env(vars(&[
            ("DATABASE_URL", "postgres://localhost/test"),
            ("JWT_SECRET", "test"),
            ("SERVER_PORT", "not-a-number"),
        ]));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("SERVER_PORT invalid"));
    }

    #[test]
    fn test_settings_new_invalid_db_max_connections() {
        let result = Settings::from_env(vars(&[
            ("DATABASE_URL", "postgres://localhost/test"),
            ("JWT_SECRET", "test"),
            ("SERVER_PORT", "3000"),
            ("DB_MAX_CONNECTIONS", "abc"),
        ]));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("DB_MAX_CONNECTIONS invalid"));
    }

    #[test]
    fn test_settings_new_defaults() {
        let settings = Settings::from_env(vars(&[
            ("DATABASE_URL", "postgres://localhost/test"),
            ("JWT_SECRET", "test"),
        ]))
        .unwrap();
        assert_eq!(settings.server_host, "0.0.0.0");
        assert_eq!(settings.server_port, 3000);
        assert_eq!(settings.db_max_connections, 5);
        assert_eq!(settings.admin_password, "admin");
        assert_eq!(settings.container_runtime, "docker");
        assert_eq!(settings.host_gateway_ip, "172.17.0.1");
        assert_eq!(settings.host_port_start, 10000);
        assert_eq!(settings.host_port_end, 20000);
    }

    #[test]
    fn test_settings_new_reads_env() {
        let prev_db = std::env::var("DATABASE_URL").ok();
        let prev_jwt = std::env::var("JWT_SECRET").ok();

        unsafe {
            std::env::set_var("DATABASE_URL", "postgres://localhost/new_test_db");
            std::env::set_var("JWT_SECRET", "new_secret");
        }

        let result = Settings::new();

        unsafe {
            if let Some(v) = prev_db {
                std::env::set_var("DATABASE_URL", v);
            } else {
                std::env::remove_var("DATABASE_URL");
            }
            if let Some(v) = prev_jwt {
                std::env::set_var("JWT_SECRET", v);
            } else {
                std::env::remove_var("JWT_SECRET");
            }
        }

        let settings = result.unwrap();
        assert_eq!(settings.database_url, "postgres://localhost/new_test_db");
        assert_eq!(settings.jwt_secret, "new_secret");
    }

    #[test]
    fn test_settings_custom_env_values() {
        let settings = Settings::from_env(vars(&[
            ("DATABASE_URL", "postgres://remote:5432/prod"),
            ("JWT_SECRET", "super-secret"),
            ("ADMIN_PASSWORD", "changeme"),
            ("SERVER_HOST", "192.168.1.100"),
            ("SERVER_PORT", "8080"),
            ("DB_MAX_CONNECTIONS", "20"),
            ("DOCKER_NETWORK", "custom-network"),
            ("OW_CONTAINER_RUNTIME", "runsc"),
        ]))
        .unwrap();

        assert_eq!(settings.database_url, "postgres://remote:5432/prod");
        assert_eq!(settings.jwt_secret, "super-secret");
        assert_eq!(settings.admin_password, "changeme");
        assert_eq!(settings.server_host, "192.168.1.100");
        assert_eq!(settings.server_port, 8080);
        assert_eq!(settings.db_max_connections, 20);
        assert_eq!(settings.docker_network, "custom-network");
        assert_eq!(settings.container_runtime, "runsc");
        assert_eq!(settings.bind_address(), "192.168.1.100:8080");
    }

    #[test]
    fn test_container_runtime_default_is_docker() {
        let settings = Settings::from_env(vars(&[
            ("DATABASE_URL", "postgres://localhost/test"),
            ("JWT_SECRET", "test"),
        ]))
        .unwrap();
        assert_eq!(settings.container_runtime, "docker");
    }

    #[test]
    fn test_container_runtime_custom_value() {
        let settings = Settings::from_env(vars(&[
            ("DATABASE_URL", "postgres://localhost/test"),
            ("JWT_SECRET", "test"),
            ("OW_CONTAINER_RUNTIME", "runsc"),
        ]))
        .unwrap();
        assert_eq!(settings.container_runtime, "runsc");
    }

    #[test]
    fn test_container_runtime_empty_string() {
        let settings = Settings::from_env(vars(&[
            ("DATABASE_URL", "postgres://localhost/test"),
            ("JWT_SECRET", "test"),
            ("OW_CONTAINER_RUNTIME", ""),
        ]))
        .unwrap();
        assert_eq!(settings.container_runtime, "");
    }

    #[test]
    fn test_host_gateway_ip_default() {
        let settings = Settings::from_env(vars(&[
            ("DATABASE_URL", "postgres://localhost/test"),
            ("JWT_SECRET", "test"),
        ]))
        .unwrap();
        assert_eq!(settings.host_gateway_ip, "172.17.0.1");
    }

    #[test]
    fn test_host_gateway_ip_custom() {
        let settings = Settings::from_env(vars(&[
            ("DATABASE_URL", "postgres://localhost/test"),
            ("JWT_SECRET", "test"),
            ("OW_HOST_GATEWAY_IP", "10.0.0.1"),
        ]))
        .unwrap();
        assert_eq!(settings.host_gateway_ip, "10.0.0.1");
    }

    #[test]
    fn test_host_port_range_custom() {
        let settings = Settings::from_env(vars(&[
            ("DATABASE_URL", "postgres://localhost/test"),
            ("JWT_SECRET", "test"),
            ("OW_HOST_PORT_START", "20000"),
            ("OW_HOST_PORT_END", "30000"),
        ]))
        .unwrap();
        assert_eq!(settings.host_port_start, 20000);
        assert_eq!(settings.host_port_end, 30000);
    }

    #[test]
    fn test_host_port_start_invalid() {
        let result = Settings::from_env(vars(&[
            ("DATABASE_URL", "postgres://localhost/test"),
            ("JWT_SECRET", "test"),
            ("OW_HOST_PORT_START", "not-a-number"),
        ]));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("OW_HOST_PORT_START invalid"));
    }

    #[test]
    fn test_host_port_end_invalid() {
        let result = Settings::from_env(vars(&[
            ("DATABASE_URL", "postgres://localhost/test"),
            ("JWT_SECRET", "test"),
            ("OW_HOST_PORT_END", "not-a-number"),
        ]));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("OW_HOST_PORT_END invalid"));
    }

    #[test]
    fn test_host_port_range_custom_env_values() {
        let settings = Settings::from_env(vars(&[
            ("DATABASE_URL", "postgres://remote:5432/prod"),
            ("JWT_SECRET", "super-secret"),
            ("OW_HOST_GATEWAY_IP", "192.168.50.1"),
            ("OW_HOST_PORT_START", "40000"),
            ("OW_HOST_PORT_END", "50000"),
        ]))
        .unwrap();
        assert_eq!(settings.host_gateway_ip, "192.168.50.1");
        assert_eq!(settings.host_port_start, 40000);
        assert_eq!(settings.host_port_end, 50000);
    }
}
