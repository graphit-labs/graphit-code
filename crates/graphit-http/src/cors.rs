//! Captured origin policy; Tower owns the CORS protocol and preserves response bodies.
//!
//! The caller selects the listener policy and mounts it before authentication.
//! CORS neither authenticates requests nor enforces the web session's CSRF rules.

use std::{fmt, net::IpAddr};

use http::{HeaderName, HeaderValue, Method, Uri, header::ORIGIN};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};

/// Listener policies are explicit: an empty MCP allowlist is not the UI default.
pub enum Origins {
    Disabled,
    LoopbackHttp,
    Any,
    Exact(Vec<HeaderValue>),
}

impl fmt::Debug for Origins {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disabled => f.write_str("Disabled"),
            Self::LoopbackHttp => f.write_str("LoopbackHttp"),
            Self::Any => f.write_str("Any"),
            Self::Exact(values) => f.debug_tuple("Exact").field(&values.len()).finish(),
        }
    }
}

/// Bounds configuration retained by the middleware and request origin parsing.
/// Caller allocations and the HTTP server's header/body limits are separate.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_entries: usize,
    /// Includes one separator byte per configured origin, method or header name.
    pub max_config_bytes: usize,
    /// Applies to Exact configuration and LoopbackHttp/Exact request origins.
    /// Any sends a constant wildcard without parsing the request origin.
    pub max_origin_bytes: usize,
}

pub struct Options {
    pub origins: Origins,
    pub methods: Vec<Method>,
    pub allowed_headers: Vec<HeaderName>,
    pub exposed_headers: Vec<HeaderName>,
    pub limits: Limits,
}

impl fmt::Debug for Options {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Options")
            .field("origins", &self.origins)
            .field("methods", &self.methods.len())
            .field("allowed_headers", &self.allowed_headers.len())
            .field("exposed_headers", &self.exposed_headers.len())
            .field("limits", &self.limits)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigError {
    InvalidLimits,
    TooManyEntries,
    TooManyBytes,
    OriginTooLong,
    InvalidOrigin,
    EmptyOrigins,
    EmptyMethods,
    WildcardField,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidLimits => "CORS limits must be positive",
            Self::TooManyEntries => "CORS entry budget exceeded",
            Self::TooManyBytes => "CORS configuration byte budget exceeded",
            Self::OriginTooLong => "CORS origin byte budget exceeded",
            Self::InvalidOrigin => {
                "CORS origin must be an HTTP(S) origin without credentials or a path"
            }
            Self::EmptyOrigins => "CORS Exact requires an origin; use Disabled for no middleware",
            Self::EmptyMethods => "enabled CORS requires explicit methods",
            Self::WildcardField => "CORS fields must be explicit; use Any for wildcard origins",
        })
    }
}

impl std::error::Error for ConfigError {}

/// Constructs an immutable middleware snapshot with credentials/PNA disabled.
/// None means no middleware at all, including no OPTIONS interception.
pub fn layer(options: Options) -> Result<Option<CorsLayer>, ConfigError> {
    validate(&options)?;
    let Options {
        origins,
        methods,
        allowed_headers,
        exposed_headers,
        limits,
    } = options;
    let allow_origin = match origins {
        Origins::Disabled => return Ok(None),
        Origins::Any => AllowOrigin::from(Any),
        origins => {
            // Own the checked bytes: a short HeaderValue can share a much larger buffer.
            let exact = match origins {
                Origins::Exact(values) => Some(
                    values
                        .iter()
                        .map(|origin| {
                            let mut copy = HeaderValue::from_bytes(origin.as_bytes())
                                .map_err(|_| ConfigError::InvalidOrigin)?;
                            copy.set_sensitive(origin.is_sensitive());
                            Ok(copy)
                        })
                        .collect::<Result<Box<[_]>, ConfigError>>()?,
                ),
                Origins::LoopbackHttp => None,
                Origins::Disabled | Origins::Any => unreachable!(),
            };
            AllowOrigin::predicate(move |origin, parts| {
                let mut values = parts.headers.get_all(ORIGIN).iter();
                if values.next().is_none()
                    || values.next().is_some()
                    || origin.as_bytes().len() > limits.max_origin_bytes
                {
                    return false;
                }
                match &exact {
                    Some(values) => values.contains(origin),
                    None => is_loopback_http(origin),
                }
            })
        }
    };
    Ok(Some(
        CorsLayer::new()
            .allow_origin(allow_origin)
            .allow_methods(methods)
            .allow_headers(allowed_headers)
            .expose_headers(exposed_headers),
    ))
}

fn validate(options: &Options) -> Result<(), ConfigError> {
    let limits = options.limits;
    if limits.max_entries == 0 || limits.max_config_bytes == 0 || limits.max_origin_bytes == 0 {
        return Err(ConfigError::InvalidLimits);
    }
    let mut entries = 0_usize;
    let mut bytes = 0_usize;
    let mut account = |length: usize| -> Result<(), ConfigError> {
        entries = entries.checked_add(1).ok_or(ConfigError::TooManyEntries)?;
        if entries > limits.max_entries {
            return Err(ConfigError::TooManyEntries);
        }
        bytes = bytes
            .checked_add(length)
            .and_then(|n| n.checked_add(1))
            .ok_or(ConfigError::TooManyBytes)?;
        if bytes > limits.max_config_bytes {
            return Err(ConfigError::TooManyBytes);
        }
        Ok(())
    };
    if let Origins::Exact(origins) = &options.origins {
        if origins.is_empty() {
            return Err(ConfigError::EmptyOrigins);
        }
        for origin in origins {
            account(origin.as_bytes().len())?;
            if origin.as_bytes().len() > limits.max_origin_bytes {
                return Err(ConfigError::OriginTooLong);
            }
            if http_origin(origin).is_none() {
                return Err(ConfigError::InvalidOrigin);
            }
        }
    }
    for method in &options.methods {
        account(method.as_str().len())?;
        if method.as_str() == "*" {
            return Err(ConfigError::WildcardField);
        }
    }
    for name in options
        .allowed_headers
        .iter()
        .chain(&options.exposed_headers)
    {
        account(name.as_str().len())?;
        if name == "*" {
            return Err(ConfigError::WildcardField);
        }
    }
    if !matches!(options.origins, Origins::Disabled) && options.methods.is_empty() {
        return Err(ConfigError::EmptyMethods);
    }
    Ok(())
}

fn http_origin(value: &HeaderValue) -> Option<Uri> {
    let raw = value.to_str().ok()?;
    let uri: Uri = raw.parse().ok()?;
    let scheme = uri.scheme_str()?;
    if !matches!(scheme, "http" | "https") {
        return None;
    }
    let authority = uri.authority()?;
    // Uri accepts paths and userinfo. An Origin has neither (not even a final slash).
    if authority.as_str().contains('@') || raw.len() != scheme.len() + 3 + authority.as_str().len()
    {
        return None;
    }
    let host = authority.host();
    if host.is_empty() {
        return None;
    }
    if host.starts_with('[') {
        host.strip_prefix('[')?
            .strip_suffix(']')?
            .parse::<std::net::Ipv6Addr>()
            .ok()?;
    }
    let suffix = authority.as_str().strip_prefix(host)?;
    if !suffix.is_empty() {
        let port = suffix.strip_prefix(':')?;
        if port.is_empty()
            || !port.bytes().all(|b| b.is_ascii_digit())
            || port.parse::<u16>().is_err()
        {
            return None;
        }
    }
    Some(uri)
}

fn is_loopback_http(origin: &HeaderValue) -> bool {
    let Some(uri) = http_origin(origin) else {
        return false;
    };
    if uri.scheme_str() != Some("http") {
        return false;
    }
    let host = uri.host().expect("validated origin has a host");
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    let host = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => ip.is_loopback(),
        Ok(IpAddr::V6(ip)) => {
            ip.is_loopback() || ip.to_ipv4_mapped().is_some_and(|ip| ip.is_loopback())
        }
        Err(_) => false,
    }
}
