use axum::http::{HeaderName, HeaderValue};
use axum::middleware;
use axum::Router;
use serde::Serialize;
use tower_http::cors::CorsLayer;
use tower_http::set_header::SetResponseHeaderLayer;

use crate::telemetry;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteClass {
    Health,
    JsonCommand,
    Upload,
    Query,
    MediaFile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Control {
    RequestId,
    BodyLimit,
    Authorization,
    RateLimit,
    Timeout,
    Tracing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Enforcement {
    Middleware,
    Extractor,
    LocalDeploymentBoundary,
    ServiceConcurrency,
    ServiceDeadline,
    NotApplicable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutePolicy {
    pub method: &'static str,
    pub path: &'static str,
    pub class: RouteClass,
    pub controls: [(Control, Enforcement); 6],
}

const fn controls(
    body: Enforcement,
    authorization: Enforcement,
    rate: Enforcement,
    timeout: Enforcement,
) -> [(Control, Enforcement); 6] {
    [
        (Control::RequestId, Enforcement::Middleware),
        (Control::BodyLimit, body),
        (Control::Authorization, authorization),
        (Control::RateLimit, rate),
        (Control::Timeout, timeout),
        (Control::Tracing, Enforcement::Middleware),
    ]
}

const LOCAL: Enforcement = Enforcement::LocalDeploymentBoundary;
const JSON: [(Control, Enforcement); 6] = controls(
    Enforcement::Extractor,
    LOCAL,
    Enforcement::ServiceConcurrency,
    Enforcement::ServiceDeadline,
);
const QUERY: [(Control, Enforcement); 6] = controls(
    Enforcement::NotApplicable,
    LOCAL,
    Enforcement::ServiceConcurrency,
    Enforcement::ServiceDeadline,
);
const HEALTH: [(Control, Enforcement); 6] = controls(
    Enforcement::NotApplicable,
    Enforcement::NotApplicable,
    Enforcement::NotApplicable,
    Enforcement::ServiceDeadline,
);
const MEDIA: [(Control, Enforcement); 6] = controls(
    Enforcement::NotApplicable,
    LOCAL,
    Enforcement::NotApplicable,
    Enforcement::NotApplicable,
);

pub const ROUTE_POLICIES: &[RoutePolicy] = &[
    RoutePolicy {
        method: "POST",
        path: "/api/import",
        class: RouteClass::JsonCommand,
        controls: JSON,
    },
    RoutePolicy {
        method: "POST",
        path: "/api/upload",
        class: RouteClass::Upload,
        controls: JSON,
    },
    RoutePolicy {
        method: "POST",
        path: "/api/edit",
        class: RouteClass::JsonCommand,
        controls: JSON,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/jobs/:id",
        class: RouteClass::Query,
        controls: QUERY,
    },
    RoutePolicy {
        method: "POST",
        path: "/api/jobs/:id/cancel",
        class: RouteClass::JsonCommand,
        controls: JSON,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/library",
        class: RouteClass::Query,
        controls: QUERY,
    },
    RoutePolicy {
        method: "DELETE",
        path: "/api/library/:id",
        class: RouteClass::JsonCommand,
        controls: JSON,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/projects",
        class: RouteClass::Query,
        controls: QUERY,
    },
    RoutePolicy {
        method: "POST",
        path: "/api/projects",
        class: RouteClass::JsonCommand,
        controls: JSON,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/projects/by-video/:videoId",
        class: RouteClass::Query,
        controls: QUERY,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/projects/:id",
        class: RouteClass::Query,
        controls: QUERY,
    },
    RoutePolicy {
        method: "DELETE",
        path: "/api/projects/:id",
        class: RouteClass::JsonCommand,
        controls: JSON,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/health",
        class: RouteClass::Health,
        controls: HEALTH,
    },
    RoutePolicy {
        method: "GET",
        path: "/api/capabilities",
        class: RouteClass::Health,
        controls: HEALTH,
    },
    RoutePolicy {
        method: "GET",
        path: "/files/*path",
        class: RouteClass::MediaFile,
        controls: MEDIA,
    },
];

pub fn policy_snapshot() -> String {
    serde_json::to_string_pretty(ROUTE_POLICIES).expect("static route policy serializes")
}

/// One outer stack for API responses and media files. Body limits remain route
/// local because upload and JSON commands have intentionally different budgets.
pub fn apply_public_layers(router: Router, cors: CorsLayer) -> Router {
    router
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("content-security-policy"),
            HeaderValue::from_static("sandbox; default-src 'none'"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("x-content-type-options"),
            HeaderValue::from_static("nosniff"),
        ))
        .layer(middleware::from_fn(telemetry::request_context))
        .layer(cors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_catalog_has_one_order_and_no_duplicate_routes() {
        let expected = [
            Control::RequestId,
            Control::BodyLimit,
            Control::Authorization,
            Control::RateLimit,
            Control::Timeout,
            Control::Tracing,
        ];
        let mut routes = std::collections::BTreeSet::new();
        for policy in ROUTE_POLICIES {
            assert_eq!(policy.controls.map(|(control, _)| control), expected);
            assert!(routes.insert((policy.method, policy.path)));
        }
        let snapshot = policy_snapshot();
        assert!(snapshot.contains("local_deployment_boundary"));
        assert!(snapshot.contains("service_deadline"));
    }
}
