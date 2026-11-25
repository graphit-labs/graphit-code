use super::*;
pub(super) struct OAuthClient<'a> {
    pub(super) verifier: &'a BrokerVerifier,
    pub(super) limits: OAuthLimits,
}
pub(super) struct LoginAuthorization {
    pub(super) url: Zeroizing<String>,
    pub(super) state: Zeroizing<String>,
    pub(super) nonce: Zeroizing<String>,
    pub(super) verifier: Zeroizing<String>,
}
impl OAuthClient<'_> {
    pub(super) fn authorization(
        &self,
        metadata: &BrokerMetadata,
        client_id: &str,
        redirect: &str,
        resource: Option<&str>,
        context: BrokerRequestContext<'_>,
    ) -> Result<LoginAuthorization, NativeError> {
        let scope_len = self.scope_length(metadata)?;
        self.scalar(client_id)?;
        self.scalar(redirect)?;
        self.scalar_len(43)?;
        let fields = [
            ("response_type", 4),
            ("client_id", client_id.len()),
            ("state", 43),
            ("code_challenge", 43),
            ("code_challenge_method", 4),
            ("redirect_uri", redirect.len()),
            ("scope", scope_len),
            ("nonce", 43),
        ];
        let mut bound = encoded_bound(fields.into_iter(), self.limits.authorization_url_bytes)?;
        if let Some(resource) = resource {
            self.scalar(resource)?;
            bound = bound
                .checked_add(
                    encoded_bound(
                        [("resource", resource.len())].into_iter(),
                        self.limits.authorization_url_bytes,
                    )? + 1,
                )
                .ok_or_else(|| NativeError::local(NativeErrorKind::Limit))?;
        }
        bound = bound
            .checked_add(metadata.authorization_endpoint().len())
            .and_then(|n| n.checked_add(1))
            .ok_or_else(|| NativeError::local(NativeErrorKind::Limit))?;
        self.cap(bound, self.limits.authorization_url_bytes)?;
        let state = random_secret()?;
        let nonce = random_secret()?;
        let verifier = random_secret()?;
        self.verifier.expected_input(&nonce)?;
        context.check()?;
        // The SDK owns bounded intermediate strings. Their allocator erasure is not promised.
        if !(43..=128).contains(&verifier.len())
            || !verifier
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-._~".contains(&b))
        {
            return Err(NativeError::local(NativeErrorKind::InvalidPolicy));
        }
        let pkce = PkceCodeVerifier::new(verifier.to_string());
        let challenge = PkceCodeChallenge::from_code_verifier_sha256(&pkce);
        let client = oauth2::basic::BasicClient::new(ClientId::new(client_id.into()))
            .set_auth_uri(
                AuthUrl::new(metadata.authorization_endpoint().into())
                    .map_err(|_| NativeError::local(NativeErrorKind::InvalidPolicy))?,
            )
            .set_redirect_uri(
                RedirectUrl::new(redirect.into())
                    .map_err(|_| NativeError::local(NativeErrorKind::InvalidRedirect))?,
            );
        let mut request = client
            .authorize_url(|| CsrfToken::new(state.to_string()))
            .set_pkce_challenge(challenge)
            .add_extra_param("nonce", nonce.as_str());
        for scope in metadata.scopes() {
            request = request.add_scope(Scope::new(scope.clone()));
        }
        if let Some(resource) = resource {
            request = request.add_extra_param("resource", resource);
        }
        let (url, _) = request.url();
        self.cap(url.as_str().len(), self.limits.authorization_url_bytes)?;
        context.check()?;
        Ok(LoginAuthorization {
            url: Zeroizing::new(url.to_string()),
            state,
            nonce,
            verifier,
        })
    }
    pub(super) async fn post(
        &self,
        endpoint: &str,
        form: &str,
        context: BrokerRequestContext<'_>,
    ) -> Result<graphit_http::client::NetworkResponse, NativeError> {
        context.check()?;
        let headers = post_headers();
        self.execute_post(endpoint, form, &headers, StatusCode::OK, context)
            .await
    }
    pub(super) async fn execute_post(
        &self,
        endpoint: &str,
        form: &str,
        headers: &HeaderMap,
        expected_status: StatusCode,
        context: BrokerRequestContext<'_>,
    ) -> Result<graphit_http::client::NetworkResponse, NativeError> {
        let response = self
            .verifier
            .client
            .execute(
                NetworkRequest {
                    method: Method::POST,
                    url: endpoint,
                    headers,
                    body: form.as_bytes(),
                },
                context.network(),
            )
            .await
            .map_err(|e| {
                if e == NetworkError::Busy {
                    NativeError::local(NativeErrorKind::Busy)
                } else {
                    NativeError::remote(NativeErrorKind::Broker(e.into()))
                }
            })?;
        if response.status() != expected_status {
            return Err(NativeError::remote(NativeErrorKind::HttpRejected));
        }
        context
            .check()
            .map_err(|e| NativeError::remote(NativeErrorKind::Broker(e)))?;
        Ok(response)
    }
    pub(super) fn scalar_len(&self, len: usize) -> Result<(), NativeError> {
        self.cap(len, self.limits.response_scalar_bytes)
    }
    pub(super) fn scalar(&self, value: &str) -> Result<(), NativeError> {
        self.scalar_len(value.len())
    }
    pub(super) fn cap(&self, len: usize, cap: usize) -> Result<(), NativeError> {
        if len > cap {
            Err(NativeError::local(NativeErrorKind::Limit))
        } else {
            Ok(())
        }
    }
    pub(super) fn token_input(&self, token: &str) -> Result<(), NativeError> {
        self.cap(token.len(), self.limits.token_bytes)?;
        self.scalar(token)?;
        if token.is_empty() {
            Err(NativeError::local(NativeErrorKind::InvalidResponse))
        } else {
            Ok(())
        }
    }
    pub(super) fn scope_length(&self, metadata: &BrokerMetadata) -> Result<usize, NativeError> {
        let mut len = 0usize;
        for (index, scope) in metadata.scopes().iter().enumerate() {
            self.scalar(scope)?;
            len = len
                .checked_add(scope.len())
                .and_then(|n| n.checked_add(usize::from(index != 0)))
                .ok_or_else(|| NativeError::local(NativeErrorKind::Limit))?;
        }
        self.scalar_len(len)?;
        Ok(len)
    }
    pub(super) fn endpoint(&self, endpoint: &str) -> Result<(), NativeError> {
        let url = checked_url(endpoint, self.verifier.url_bytes)?;
        if url.origin() != self.verifier.origin {
            return Err(NativeError::local(NativeErrorKind::InvalidPolicy));
        }
        let mut count = 0usize;
        for (key, value) in url.query_pairs() {
            count = count
                .checked_add(1)
                .ok_or_else(|| NativeError::local(NativeErrorKind::Limit))?;
            self.cap(count, self.limits.callback_query_entries)?;
            self.scalar(&key)?;
            self.scalar(&value)?;
            if matches!(
                key.as_ref(),
                "client_id"
                    | "redirect_uri"
                    | "response_type"
                    | "scope"
                    | "state"
                    | "nonce"
                    | "code_challenge"
                    | "code_challenge_method"
                    | "resource"
                    | "grant_type"
                    | "code"
                    | "code_verifier"
                    | "refresh_token"
                    | "token"
                    | "token_type_hint"
            ) {
                return Err(NativeError::local(NativeErrorKind::InvalidPolicy));
            }
        }
        Ok(())
    }
    pub(super) fn redirect(&self, text: &str) -> Result<Url, NativeError> {
        let url = checked_url(text, self.limits.callback_bytes)?;
        let uri: http::Uri = text
            .parse()
            .map_err(|_| NativeError::local(NativeErrorKind::InvalidRedirect))?;
        let authority = uri
            .authority()
            .ok_or_else(|| NativeError::local(NativeErrorKind::InvalidRedirect))?;
        let host = authority.host();
        let host = host
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
            .unwrap_or(host);
        let ip: std::net::IpAddr = host
            .parse()
            .map_err(|_| NativeError::local(NativeErrorKind::InvalidRedirect))?;
        if url.scheme() != "http"
            || !ip.is_loopback()
            || !literal_loopback(&url)
            || authority.port_u16().is_none_or(|n| n == 0)
            || url.query().is_some()
        {
            return Err(NativeError::local(NativeErrorKind::InvalidRedirect));
        }
        Ok(url)
    }
    pub(super) fn form(
        &self,
        fields: &[(&str, &str)],
        context: BrokerRequestContext<'_>,
    ) -> Result<Zeroizing<String>, NativeError> {
        context.check()?;
        for (_, value) in fields {
            self.scalar(value)?;
        }
        let bound = encoded_bound(
            fields.iter().map(|(key, value)| (*key, value.len())),
            self.limits.form_body_bytes,
        )?;
        let mut serializer = form_urlencoded::Serializer::new(String::with_capacity(bound));
        serializer.extend_pairs(fields.iter().copied());
        let body = Zeroizing::new(serializer.finish());
        self.cap(body.len(), self.limits.form_body_bytes)?;
        context.check()?;
        Ok(body)
    }
    pub(super) fn callback(
        &self,
        redirect: &str,
        state: &str,
        text: &str,
        context: BrokerRequestContext<'_>,
    ) -> Result<Zeroizing<String>, NativeError> {
        context.check()?;
        let url = checked_url(text, self.limits.callback_bytes)?;
        let redirect_url = checked_url(redirect, self.limits.callback_bytes)?;
        let uri: http::Uri = text
            .parse()
            .map_err(|_| NativeError::local(NativeErrorKind::InvalidCallback))?;
        let redirect_uri: http::Uri = redirect
            .parse()
            .map_err(|_| NativeError::local(NativeErrorKind::InvalidCallback))?;
        if url.origin() != redirect_url.origin()
            || url.path() != redirect_url.path()
            || uri.path() != redirect_uri.path()
        {
            return Err(NativeError::local(NativeErrorKind::InvalidCallback));
        }
        let mut seen = 0u8;
        let mut count = 0usize;
        let mut code = None;
        let mut state_matches = false;
        for (key, value) in url.query_pairs() {
            context.check()?;
            count = count
                .checked_add(1)
                .ok_or_else(|| NativeError::local(NativeErrorKind::Limit))?;
            self.cap(count, self.limits.callback_query_entries)?;
            self.scalar(&key)?;
            self.scalar(&value)?;
            let bit = match key.as_ref() {
                "code" => 1,
                "state" => 2,
                "error" => 4,
                "error_description" => 8,
                "error_uri" => 16,
                _ => return Err(NativeError::local(NativeErrorKind::InvalidCallback)),
            };
            if seen & bit != 0 || (value.is_empty() && bit & 7 != 0) {
                return Err(NativeError::local(NativeErrorKind::InvalidCallback));
            }
            seen |= bit;
            if bit == 1 {
                code = Some(Zeroizing::new(value.into_owned()));
            } else if bit == 2 {
                state_matches = bool::from(value.as_bytes().ct_eq(state.as_bytes()));
            }
        }
        if !state_matches
            || seen & 2 == 0
            || (seen & 1 != 0) == (seen & 4 != 0)
            || (seen & 24 != 0 && seen & 4 == 0)
        {
            return Err(NativeError::local(NativeErrorKind::InvalidCallback));
        }
        if seen & 4 != 0 {
            return Err(NativeError::local(NativeErrorKind::AuthorizationRejected));
        }
        code.ok_or_else(|| NativeError::local(NativeErrorKind::InvalidCallback))
    }
}
