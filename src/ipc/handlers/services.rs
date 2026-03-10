//! Service registry IPC handler — registration, discovery, health, load.

use crate::ipc::router::{str_field, DispatchResponse};
use crate::kernel::Kernel;
use crate::types::{Error, Result};
use serde_json::Value;

pub async fn handle(kernel: &mut Kernel, method: &str, body: Value) -> Result<DispatchResponse> {
    match method {
        "RegisterService" => {
            let name = str_field(&body, "name")?;
            let service_type = str_field(&body, "service_type")?;

            let mut info = crate::kernel::ServiceInfo::new(name, service_type);

            if let Some(v) = body.get("version").and_then(|v| v.as_str()) {
                info.version = v.to_string();
            }
            if let Some(caps) = body.get("capabilities").and_then(|v| v.as_array()) {
                info.capabilities = caps
                    .iter()
                    .filter_map(|c| c.as_str().map(|s| s.to_string()))
                    .collect();
            }
            if let Some(mc) = body.get("max_concurrent").and_then(|v| v.as_i64()) {
                info.max_concurrent = mc as i32;
            }
            if let Some(meta) = body.get("metadata").and_then(|v| v.as_object()) {
                for (k, v) in meta {
                    info.metadata.insert(k.clone(), v.clone());
                }
            }

            let registered = kernel.register_service(info);
            Ok(DispatchResponse::Single(serde_json::json!({
                "registered": registered,
            })))
        }

        "UnregisterService" => {
            let name = str_field(&body, "name")?;
            let unregistered = kernel.unregister_service(&name);
            Ok(DispatchResponse::Single(serde_json::json!({
                "unregistered": unregistered,
            })))
        }

        "GetService" => {
            let name = str_field(&body, "name")?;
            match kernel.get_service(&name) {
                Some(info) => Ok(DispatchResponse::Single(
                    serde_json::to_value(&info).unwrap_or_default(),
                )),
                None => Ok(DispatchResponse::Single(Value::Null)),
            }
        }

        "ListServices" => {
            let service_type = body.get("service_type").and_then(|v| v.as_str());
            let healthy_only = body
                .get("healthy_only")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let services = kernel.list_services(service_type, healthy_only);
            let serialized: Vec<Value> = services
                .iter()
                .map(|s| serde_json::to_value(s).unwrap_or_default())
                .collect();
            Ok(DispatchResponse::Single(serde_json::json!({
                "services": serialized,
            })))
        }

        "GetServiceNames" => {
            let names = kernel.get_service_names();
            Ok(DispatchResponse::Single(serde_json::json!({
                "names": names,
            })))
        }

        "UpdateHealth" => {
            let name = str_field(&body, "name")?;
            let status_str = str_field(&body, "status")?;
            let status = crate::ipc::handlers::validation::parse_enum::<
                crate::kernel::ServiceStatus,
            >(&status_str, "status")?;

            let updated = kernel.update_service_health(&name, status);
            Ok(DispatchResponse::Single(serde_json::json!({
                "updated": updated,
            })))
        }

        "IncrementLoad" => {
            let name = str_field(&body, "name")?;
            let success = kernel.increment_service_load(&name);
            Ok(DispatchResponse::Single(serde_json::json!({
                "success": success,
            })))
        }

        "DecrementLoad" => {
            let name = str_field(&body, "name")?;
            let success = kernel.decrement_service_load(&name);
            Ok(DispatchResponse::Single(serde_json::json!({
                "success": success,
            })))
        }

        "GetLoad" => {
            let name = str_field(&body, "name")?;
            let load = kernel.get_service_load(&name);
            Ok(DispatchResponse::Single(serde_json::json!({
                "load": load,
            })))
        }

        "GetServiceStats" => {
            let name = str_field(&body, "name")?;
            match kernel.get_service_stats(&name) {
                Some(stats) => Ok(DispatchResponse::Single(
                    serde_json::to_value(&stats).unwrap_or_default(),
                )),
                None => Ok(DispatchResponse::Single(Value::Null)),
            }
        }

        "GetRegistryStats" => {
            let stats = kernel.get_registry_stats();
            Ok(DispatchResponse::Single(
                serde_json::to_value(&stats).unwrap_or_default(),
            ))
        }

        _ => Err(Error::not_found(format!(
            "Unknown services method: {}",
            method
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::Kernel;

    async fn call(kernel: &mut Kernel, method: &str, body: Value) -> Value {
        match handle(kernel, method, body).await.unwrap() {
            DispatchResponse::Single(v) => v,
            _ => panic!("Expected Single response"),
        }
    }

    #[tokio::test]
    async fn test_register_and_get_service() {
        let mut kernel = Kernel::new();
        let result = call(
            &mut kernel,
            "RegisterService",
            serde_json::json!({
                "name": "my_flow",
                "service_type": "flow",
                "version": "1.0.0",
                "capabilities": ["route", "transform"],
                "max_concurrent": 5,
            }),
        )
        .await;
        assert_eq!(result["registered"], true);

        let svc = call(
            &mut kernel,
            "GetService",
            serde_json::json!({"name": "my_flow"}),
        )
        .await;
        assert_eq!(svc["name"], "my_flow");
        assert_eq!(svc["service_type"], "flow");
        assert_eq!(svc["version"], "1.0.0");
        assert_eq!(svc["max_concurrent"], 5);
    }

    #[tokio::test]
    async fn test_register_duplicate_returns_false() {
        let mut kernel = Kernel::new();
        let body = serde_json::json!({"name": "svc1", "service_type": "worker"});
        let r1 = call(&mut kernel, "RegisterService", body.clone()).await;
        assert_eq!(r1["registered"], true);
        let r2 = call(&mut kernel, "RegisterService", body).await;
        assert_eq!(r2["registered"], false);
    }

    #[tokio::test]
    async fn test_list_services_with_filters() {
        let mut kernel = Kernel::new();
        call(
            &mut kernel,
            "RegisterService",
            serde_json::json!({"name": "f1", "service_type": "flow"}),
        )
        .await;
        call(
            &mut kernel,
            "RegisterService",
            serde_json::json!({"name": "f2", "service_type": "flow"}),
        )
        .await;
        call(
            &mut kernel,
            "RegisterService",
            serde_json::json!({"name": "w1", "service_type": "worker"}),
        )
        .await;

        // Update f2 to unhealthy
        call(
            &mut kernel,
            "UpdateHealth",
            serde_json::json!({"name": "f2", "status": "unhealthy"}),
        )
        .await;

        // All services
        let all = call(
            &mut kernel,
            "ListServices",
            serde_json::json!({}),
        )
        .await;
        assert_eq!(all["services"].as_array().unwrap().len(), 3);

        // Only flow
        let flows = call(
            &mut kernel,
            "ListServices",
            serde_json::json!({"service_type": "flow"}),
        )
        .await;
        assert_eq!(flows["services"].as_array().unwrap().len(), 2);

        // Only healthy
        let healthy = call(
            &mut kernel,
            "ListServices",
            serde_json::json!({"healthy_only": true}),
        )
        .await;
        assert_eq!(healthy["services"].as_array().unwrap().len(), 2);

        // Healthy flow only
        let hf = call(
            &mut kernel,
            "ListServices",
            serde_json::json!({"service_type": "flow", "healthy_only": true}),
        )
        .await;
        assert_eq!(hf["services"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_unregister_then_get_null() {
        let mut kernel = Kernel::new();
        call(
            &mut kernel,
            "RegisterService",
            serde_json::json!({"name": "svc1", "service_type": "worker"}),
        )
        .await;

        let r = call(
            &mut kernel,
            "UnregisterService",
            serde_json::json!({"name": "svc1"}),
        )
        .await;
        assert_eq!(r["unregistered"], true);

        let svc = call(
            &mut kernel,
            "GetService",
            serde_json::json!({"name": "svc1"}),
        )
        .await;
        assert!(svc.is_null());
    }

    #[tokio::test]
    async fn test_load_round_trip() {
        let mut kernel = Kernel::new();
        call(
            &mut kernel,
            "RegisterService",
            serde_json::json!({"name": "svc1", "service_type": "worker"}),
        )
        .await;

        call(
            &mut kernel,
            "IncrementLoad",
            serde_json::json!({"name": "svc1"}),
        )
        .await;
        call(
            &mut kernel,
            "IncrementLoad",
            serde_json::json!({"name": "svc1"}),
        )
        .await;

        let load = call(
            &mut kernel,
            "GetLoad",
            serde_json::json!({"name": "svc1"}),
        )
        .await;
        assert_eq!(load["load"], 2);

        call(
            &mut kernel,
            "DecrementLoad",
            serde_json::json!({"name": "svc1"}),
        )
        .await;

        let load = call(
            &mut kernel,
            "GetLoad",
            serde_json::json!({"name": "svc1"}),
        )
        .await;
        assert_eq!(load["load"], 1);
    }

    #[tokio::test]
    async fn test_update_health_and_stats() {
        let mut kernel = Kernel::new();
        call(
            &mut kernel,
            "RegisterService",
            serde_json::json!({"name": "svc1", "service_type": "worker"}),
        )
        .await;

        call(
            &mut kernel,
            "UpdateHealth",
            serde_json::json!({"name": "svc1", "status": "degraded"}),
        )
        .await;

        let stats = call(
            &mut kernel,
            "GetServiceStats",
            serde_json::json!({"name": "svc1"}),
        )
        .await;
        assert_eq!(stats["status"], "degraded");
        assert_eq!(stats["name"], "svc1");
    }

    #[tokio::test]
    async fn test_get_registry_stats() {
        let mut kernel = Kernel::new();
        call(
            &mut kernel,
            "RegisterService",
            serde_json::json!({"name": "f1", "service_type": "flow"}),
        )
        .await;
        call(
            &mut kernel,
            "RegisterService",
            serde_json::json!({"name": "w1", "service_type": "worker"}),
        )
        .await;

        let stats = call(&mut kernel, "GetRegistryStats", serde_json::json!({})).await;
        assert_eq!(stats["total_services"], 2);
        assert_eq!(stats["healthy_services"], 2);
    }

    #[tokio::test]
    async fn test_get_service_names() {
        let mut kernel = Kernel::new();
        call(
            &mut kernel,
            "RegisterService",
            serde_json::json!({"name": "alpha", "service_type": "flow"}),
        )
        .await;
        call(
            &mut kernel,
            "RegisterService",
            serde_json::json!({"name": "beta", "service_type": "worker"}),
        )
        .await;

        let result = call(&mut kernel, "GetServiceNames", serde_json::json!({})).await;
        let names = result["names"].as_array().unwrap();
        assert_eq!(names.len(), 2);
    }

    #[tokio::test]
    async fn test_unknown_method_returns_error() {
        let mut kernel = Kernel::new();
        let result = handle(&mut kernel, "Bogus", serde_json::json!({})).await;
        assert!(result.is_err());
    }
}
