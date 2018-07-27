// FIXME: uncomment
//#![warn(missing_docs)]

// FIXME: remove
//#![feature(trace_macros)]

//!
//! [OpenID Connect](http://openid.net/specs/openid-connect-core-1_0.html) support.
//!

// FIXME: specify the backward compatibility contract (e.g., no guarantee that non-JSON
// serializations will continue to deserialize; fields may be reordered, so assuming a particular
// order is undefined behavior).

extern crate base64;
extern crate chrono;
extern crate curl;
extern crate failure;
#[macro_use] extern crate failure_derive;
#[macro_use] extern crate log;
extern crate oauth2;
extern crate rand;
extern crate ring;
extern crate serde;
#[macro_use] extern crate serde_derive;
extern crate serde_json;
extern crate untrusted;
extern crate url;

use std::collections::HashMap;
use std::fmt::{Debug, Formatter, Result as FormatterResult};
use std::marker::PhantomData;
use std::ops::Deref;
use std::str;
use std::time::Duration;

use chrono::{DateTime, TimeZone, Utc};
use oauth2::prelude::*;
use oauth2::{
    AccessToken,
    AuthorizationCode,
    AuthType,
    AuthUrl,
    ClientId,
    ClientSecret,
    CsrfToken,
    ErrorResponseType,
    ExtraTokenFields,
    RedirectUrl,
    RequestTokenError,
    ResponseType as OAuth2ResponseType,
    Scope,
    TokenResponse,
    TokenType,
    TokenUrl,
};
use oauth2::helpers::{deserialize_url, serialize_url, variant_name};
use serde::{Serialize, Serializer};
use serde::de::{Deserialize, DeserializeOwned, Deserializer, MapAccess, Visitor};
use serde::ser::SerializeMap;
use types::helpers::split_language_tag_key;
use url::Url;

use discovery::{DiscoveryError, ProviderMetadata};
use http::{
    ACCEPT_JSON,
    auth_bearer,
    HTTP_STATUS_OK,
    HttpRequest,
    HttpRequestMethod,
    MIME_TYPE_JSON,
    MIME_TYPE_JWT,
};
use jwt::{JsonWebToken, JsonWebTokenAccess, JsonWebTokenAlgorithm, JsonWebTokenHeader};
use registration::ClientRegistrationResponse;
// Flatten the module hierarchy involving types. They're only separated to improve code
// organization.
pub use types::*;
pub use verification::{
    ClaimsVerificationError,
    IdTokenVerifier,
    SignatureVerificationError,
    UserInfoVerifier,
};
use verification::{
    AudiencesClaim,
    IssuerClaim,
};

// Defined first since other modules need the macros, and definition order is significant for
// macros. This module is private.
#[macro_use] mod macros;

pub mod core;
pub mod discovery;
pub mod registration;

// Private module for HTTP(S) utilities.
mod http;

// Private module for JWT utilities.
mod jwt;

// Private module since we may move types between different modules; these are exported publicly
// via the pub use above.
mod types;

// Also private but re-exported above.
mod verification;

const CONFIG_URL_SUFFIX: &str = ".well-known/openid-configuration";
const OPENID_SCOPE: &str = "openid";


pub struct Client<AC, D, GC, JE, JS, JT, P, TE, TT>
where AC: AdditionalClaims,
        D: AuthDisplay,
        GC: GenderClaim,
        JE: JweContentEncryptionAlgorithm,
        JS: JwsSigningAlgorithm<JT>,
        JT: JsonWebKeyType,
        P: AuthPrompt,
        TE: ErrorResponseType,
        TT: TokenType {
    oauth2_client: oauth2::Client<IdTokenFields<AC, GC, JE, JS, JT>, TT, TE>,
    acr_values: Option<Vec<AuthenticationContextClass>>,
    claims_locales: Option<Vec<LanguageTag>>,
    display: Option<D>,
    max_age: Option<Duration>,
    prompts: Option<Vec<P>>,
    ui_locales: Option<Vec<LanguageTag>>,
    _phantom_jt: PhantomData<JT>,
    // FIXME: Other parameters MAY be sent. See Sections 3.2.2, 3.3.2, 5.2, 5.5, 6, and 7.2.1 for
    // additional Authorization Request parameters and parameter values defined by this
    // specification.
}
impl<AC, D, GC, JE, JS, JT, P, TE, TT> Client<AC, D, GC, JE, JS, JT, P, TE, TT>
where AC: AdditionalClaims,
        D: AuthDisplay,
        GC: GenderClaim,
        JE: JweContentEncryptionAlgorithm,
        JS: JwsSigningAlgorithm<JT>,
        JT: JsonWebKeyType,
        P: AuthPrompt,
        TE: ErrorResponseType,
        TT: TokenType {
    pub fn new(
        client_id: ClientId,
        client_secret: Option<ClientSecret>,
        auth_url: AuthUrl,
        token_url: Option<TokenUrl>
    ) -> Client<AC, D, GC, JE, JS, JT, P, TE, TT> {
        let oauth2_client =
            oauth2::Client::new(client_id, client_secret, auth_url, token_url)
                .add_scope(Scope::new(OPENID_SCOPE.to_string()));
        Client {
            oauth2_client,
            acr_values: None,
            claims_locales: None,
            display: None,
            max_age: None,
            prompts: None,
            ui_locales: None,
            _phantom_jt: PhantomData,
        }
    }

    pub fn from_dynamic_registration<AD, AT, CA, CN, CR, CT, G, JK, JU, K, PM, RM, RT, S>(
        provider_metadata: &PM,
        registration_response: &CR
    ) -> Client<AC, D, GC, JE, JS, JT, P, TE, TT>
    where AD: AuthDisplay,
          AT: ApplicationType,
          CA: ClientAuthMethod,
          CN: ClaimName,
          CR: ClientRegistrationResponse<AT, CA, G, JE, JK, JS, JT, JU, K, RT, S>,
          CT: ClaimType,
          G: GrantType,
          JK: JweKeyManagementAlgorithm,
          JU: JsonWebKeyUse,
          K: JsonWebKey<JS, JT, JU>,
          PM: ProviderMetadata<AD, CA, CN, CT, G, JE, JK, JS, JT, RM, RT, S>,
          RM: ResponseMode,
          RT: ResponseType,
          S: SubjectIdentifierType {
        Self::new(
            registration_response.client_id().clone(),
            registration_response.client_secret().cloned(),
            provider_metadata.authorization_endpoint().clone(),
            provider_metadata.token_endpoint().cloned(),
        )
    }

    ///
    /// Appends a new scope to the authorization URL.
    ///
    pub fn add_scope(mut self, scope: Scope) -> Self {
        self.oauth2_client = self.oauth2_client.add_scope(scope);
        self
    }

    ///
    /// Configures the type of client authentication used for communicating with the authorization
    /// server.
    ///
    /// The default is to use HTTP Basic authentication, as recommended in
    /// [Section 2.3.1 of RFC 6749](https://tools.ietf.org/html/rfc6749#section-2.3.1).
    ///
    pub fn set_auth_type(mut self, auth_type: AuthType) -> Self {
        self.oauth2_client = self.oauth2_client.set_auth_type(auth_type);
        self
    }

    ///
    /// Sets the the redirect URL used by the authorization endpoint.
    ///
    pub fn set_redirect_uri(mut self, redirect_uri: RedirectUrl) -> Self {
        self.oauth2_client = self.oauth2_client.set_redirect_url(redirect_uri);
        self
    }

    pub fn acr_values(&self) -> Option<&Vec<AuthenticationContextClass>> {
        self.acr_values.as_ref()
    }
    pub fn set_acr_values(mut self, acr_values: Option<Vec<AuthenticationContextClass>>) -> Self {
        self.acr_values = acr_values;
        self
    }

    pub fn claims_locales(&self) -> Option<&Vec<LanguageTag>> {
        self.claims_locales.as_ref()
    }
    pub fn set_claims_locales(mut self, claims_locales: Option<Vec<LanguageTag>>) -> Self {
        self.claims_locales = claims_locales;
        self
    }

    pub fn display(&self) -> Option<&D> {
        self.display.as_ref()
    }
    pub fn set_display(mut self, display: Option<D>) -> Self {
        self.display = display;
        self
    }

    pub fn max_age(&self) -> Option<&Duration> {
        self.max_age.as_ref()
    }
    pub fn set_max_age(mut self, max_age: Option<Duration>) -> Self {
        self.max_age = max_age;
        self
    }

    pub fn prompts(&self) -> Option<&Vec<P>> {
        self.prompts.as_ref()
    }
    pub fn set_prompts(mut self, prompts: Option<Vec<P>>) -> Self {
        self.prompts = prompts;
        self
    }

    pub fn ui_locales(&self) -> Option<&Vec<LanguageTag>> {
        self.ui_locales.as_ref()
    }
    pub fn set_ui_locales(mut self, ui_locales: Option<Vec<LanguageTag>>) -> Self {
        self.ui_locales = ui_locales;
        self
    }

    pub fn authorize_url<NF, RT, SF>(
        &self,
        authentication_flow: &AuthenticationFlow<RT>,
        state_fn: SF,
        nonce_fn: NF,
    ) -> (Url, CsrfToken, Nonce)
    where NF: Fn() -> Nonce,
          RT: ResponseType,
          SF: Fn() -> CsrfToken {
        self.authorize_url_with_hint(authentication_flow, state_fn, nonce_fn, None, None)
    }

    pub fn authorize_url_with_hint<NF, RT, SF>(
        &self,
        authentication_flow: &AuthenticationFlow<RT>,
        state_fn: SF,
        nonce_fn: NF,
        id_token_hint_opt: Option<&IdToken<AC, GC, JE, JS, JT>>,
        login_hint_opt: Option<&LoginHint>,
    ) -> (Url, CsrfToken, Nonce)
    where NF: Fn() -> Nonce,
          RT: ResponseType,
          SF: Fn() -> CsrfToken {
        // Create string versions of any options that need to be converted. This must be done
        // before creating extra_params so that the lifetimes extend beyond extra_params's lifetime.
        let acr_values_opt = join_optional_vec(self.acr_values());
        let claims_locales_opt = join_optional_vec(self.claims_locales());
        let max_age_opt = self.max_age().map(|max_age| max_age.as_secs().to_string());
        let prompts_opt = join_optional_vec(self.prompts());
        let ui_locales_opt = join_optional_vec(self.ui_locales());

        let state = state_fn();
        let nonce = nonce_fn();

        let url = {
            let mut extra_params: Vec<(&str, &str)> = vec![
                ("state", state.secret()),
                ("nonce", nonce.secret()),
            ];


            if let Some(ref acr_values) = acr_values_opt {
                extra_params.push(("acr_values", acr_values));
            }

            if let Some(ref claims_locales) = claims_locales_opt {
                extra_params.push(("claims_locales", claims_locales));
            }

            if let Some(display) = self.display() {
                extra_params.push(("display", display.to_str()));
            }

// FIXME: uncomment
/*
            if let Some(id_token_hint) = id_token_hint_opt {
                extra_params.push(("id_token_hint", id_token_hint));
            }
*/

            if let Some(login_hint) = login_hint_opt {
                extra_params.push(("login_hint", login_hint.secret()));
            }

            if let Some(ref max_age) = max_age_opt {
                extra_params.push(("max_age", max_age));
            }

            if let Some(ref prompts) = prompts_opt {
                extra_params.push(("prompt", prompts));
            }

            if let Some(ref ui_locales) = ui_locales_opt {
                extra_params.push(("ui_locales", ui_locales));
            }

            let response_type =
                match *authentication_flow {
                    AuthenticationFlow::AuthorizationCode =>
                        core::CoreResponseType::Code.to_oauth2(),
                    AuthenticationFlow::Implicit(include_token) => {
                        if include_token {
                            OAuth2ResponseType::new(
                                vec![core::CoreResponseType::IdToken, core::CoreResponseType::Token]
                                    .iter()
                                    .map(variant_name)
                                    .collect::<Vec<_>>()
                                    .join(" ")
                            )
                        } else {
                            core::CoreResponseType::IdToken.to_oauth2()
                        }
                    },
                    AuthenticationFlow::Hybrid(ref response_types) => {
                        OAuth2ResponseType::new(
                            response_types
                                .iter()
                                .map(variant_name)
                                .collect::<Vec<_>>()
                                .join(" ")
                        )
                    }
                };

            self.oauth2_client.authorize_url_extension(&response_type, &extra_params)
        };
        (url, state, nonce)
    }

    pub fn exchange_code(
        &self,
        code: AuthorizationCode
    ) -> Result<TokenResponse<IdTokenFields<AC, GC, JE, JS, JT>, TT>, RequestTokenError<TE>> {
        self.oauth2_client.exchange_code(code)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AddressClaim {
    formatted: Option<FormattedAddress>,
    street_address: Option<StreetAddress>,
    locality: Option<AddressLocality>,
    region: Option<AddressRegion>,
    postal_code: Option<AddressPostalCode>,
    country: Option<AddressCountry>,
}
impl AddressClaim {
    field_getters![
        pub self [self] {
            formatted[Option<FormattedAddress>],
            street_address[Option<StreetAddress>],
            locality[Option<AddressLocality>],
            region[Option<AddressRegion>],
            postal_code[Option<AddressPostalCode>],
            country[Option<AddressCountry>],
        }
    ];
}

///
/// Authentication flow, which determines how the Authorization Server returns the OpenID Connect
/// ID token and OAuth2 access token to the Relying Party.
///
#[derive(Clone, Debug, PartialEq)]
pub enum AuthenticationFlow<RT: ResponseType> {
    ///
    /// Authorization Code Flow.
    ///
    /// The authorization server will return an OAuth2 authorization code. Clients must subsequently
    /// call `[FIXME: specify function]` with the authorization code in order to retrieve an
    /// OpenID Connect ID token and OAuth2 access token.
    ///
    AuthorizationCode,
    ///
    /// Implicit Flow.
    ///
    /// Boolean value indicates whether an OAuth2 access token should also be returned. If `true`,
    /// the Authorization Server will return both an OAuth2 access token and OpenID Connect ID
    /// token. If `false`, it will return only an OpenID Connect ID token.
    ///
    Implicit(bool),
    ///
    /// Hybrid Flow.
    ///
    /// A hybrid flow according to [OAuth 2.0 Multiple Response Type Encoding Practices](
    ///     http://openid.net/specs/oauth-v2-multiple-response-types-1_0.html). The enum value
    /// contains the desired `response_type`s. See
    /// [Section 3](http://openid.net/specs/openid-connect-core-1_0.html#Authentication) for
    /// details.
    ///
    Hybrid(Vec<RT>),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct EmptyAdditionalClaims {}
impl AdditionalClaims for EmptyAdditionalClaims {}

// FIXME: remove this wrapper layer, and have the functions that return IdToken currently
// directly call claims() to perform the verification and extract the result. There's nothing
// a caller can do with this IdToken other than call claims() on it, so we might as well
// do that automatically. If there's ever a reasonable use case for wanting to do lower
// level stuff, we could always expose another interface that returns something like this.
// For now, let's optimize for ease of (secure) use.
#[derive(Clone, Debug, PartialEq)]
pub struct IdToken<
    AC: AdditionalClaims,
    GC: GenderClaim,
    JE: JweContentEncryptionAlgorithm,
    JS: JwsSigningAlgorithm<JT>,
    JT: JsonWebKeyType,
>(
    JsonWebToken<IdTokenClaims<AC, GC>, JE, JS, JT>
);
impl<AC, GC, JE, JS, JT> IdToken<AC, GC, JE, JS, JT>
where AC: AdditionalClaims,
        GC: GenderClaim,
        JE: JweContentEncryptionAlgorithm,
        JS: JwsSigningAlgorithm<JT>,
        JT: JsonWebKeyType {
    pub fn claims<JU, K>(
        &self,
        verifier: &IdTokenVerifier<JS, JT, JU, K>,
        nonce: &Nonce,
    ) -> Result<&IdTokenClaims<AC, GC>, ClaimsVerificationError>
    where JU: JsonWebKeyUse, K: JsonWebKey<JS, JT, JU> {
        verifier.verified_claims(&self.0, Some(nonce))
    }
}

// This is an annoying hack to work around the fact that Serde won't handle a tuple struct with more
// than one element (to accomodate the PhantomData fields) as a String.
// FIXME: remove this now that we don't have PhantomData?
mod serde_id_token {
    use serde::{Serialize, Serializer};
    use serde::de::{Deserialize, Deserializer};

    use super::{
        AdditionalClaims,
        GenderClaim,
        IdToken,
        IdTokenClaims,
        JsonWebKeyType,
        JweContentEncryptionAlgorithm,
        JwsSigningAlgorithm,
    };
    use super::jwt::JsonWebToken;

    pub fn deserialize<'de, AC, D, GC, JE, JS, JT>(
        deserializer: D
    ) -> Result<IdToken<AC, GC, JE, JS, JT>, D::Error>
    where AC: AdditionalClaims,
            D: Deserializer<'de>,
            GC: GenderClaim,
            JE: JweContentEncryptionAlgorithm,
            JS: JwsSigningAlgorithm<JT>,
            JT: JsonWebKeyType {
        Ok(IdToken(JsonWebToken::<IdTokenClaims<AC, GC>, JE, JS, JT>::deserialize(deserializer)?))
    }

    pub fn serialize<AC, GC, JE, JS, JT, S>(
        id_token: &IdToken<AC, GC, JE, JS, JT>,
        serializer: S
    ) -> Result<S::Ok, S::Error>
    where AC: AdditionalClaims,
            GC: GenderClaim,
            JE: JweContentEncryptionAlgorithm,
            JS: JwsSigningAlgorithm<JT>,
            JT: JsonWebKeyType,
            S: Serializer {
        id_token.0.serialize(serializer)
    }
}

// FIXME: document at the module level that we do not support aggregated or distributed claims,
// which are OPTIONAL in the spec:
// http://openid.net/specs/openid-connect-core-1_0.html#AggregatedDistributedClaims
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct IdTokenClaims<AC, GC>
where AC: AdditionalClaims, GC: GenderClaim {
    iss: IssuerUrl,
    // FIXME: this needs to be a vector, but it may also come as a single string
    aud: Vec<Audience>,
    exp: Seconds,
    iat: Seconds,
    auth_time: Option<Seconds>,
    nonce: Option<Nonce>,
    acr: Option<AuthenticationContextClass>,
    amr: Option<Vec<AuthenticationMethodReference>>,
    azp: Option<ClientId>,
    at_hash: Option<AccessTokenHash>,
    c_hash: Option<AuthorizationCodeHash>,

    #[serde(bound = "GC: GenderClaim")]
    #[serde(flatten)]
    standard_claims: StandardClaimsImpl<GC>,

    #[serde(bound = "AC: AdditionalClaims")]
    #[serde(flatten)]
    additional_claims: AC
}
// FIXME: see what other structs should have friendlier trait interfaces like this one
impl<AC, GC> IdTokenClaims<AC, GC>
where AC: AdditionalClaims, GC: GenderClaim {
    pub fn issuer(&self) -> &IssuerUrl { &self.iss }
    pub fn audiences(&self) -> &Vec<Audience> { &self.aud }
    pub fn expiration(&self) -> Result<DateTime<Utc>, ()> {
        Utc.timestamp_opt(*(&self.exp as &u64) as i64, 0).single().ok_or(())
    }
    pub fn issue_time(&self) -> Result<DateTime<Utc>, ()> {
        Utc.timestamp_opt(*(&self.iat as &u64) as i64, 0).single().ok_or(())
    }
    pub fn auth_time(&self) -> Option<Result<DateTime<Utc>, ()>> {
        self.auth_time
            .as_ref()
            .map(|seconds| Utc.timestamp_opt(*(seconds as &u64) as i64, 0).single().ok_or(()))
    }
    pub fn nonce(&self) -> Option<&Nonce> { self.nonce.as_ref() }
    pub fn auth_context_ref(&self) -> Option<&AuthenticationContextClass> { self.acr.as_ref() }
    pub fn auth_methods_refs(&self) -> Option<&Vec<AuthenticationMethodReference>> {
        self.amr.as_ref()
    }
    pub fn authorized_party(&self) -> Option<&ClientId> { self.azp.as_ref() }
    pub fn access_token_hash(&self) -> Option<&AccessTokenHash> { self.at_hash.as_ref() }
    pub fn code_hash(&self) -> Option<&AuthorizationCodeHash> { self.c_hash.as_ref() }

    pub fn additional_claims(&self) -> &AC { &self.additional_claims }
}
impl<AC, GC> StandardClaims<GC> for IdTokenClaims<AC, GC>
where AC: AdditionalClaims, GC: GenderClaim {
    field_getters![
        self [self.standard_claims] {
            sub[SubjectIdentifier],
            name[Option<HashMap<Option<LanguageTag>, EndUserName>>],
            given_name[Option<HashMap<Option<LanguageTag>, EndUserGivenName>>],
            family_name[Option<HashMap<Option<LanguageTag>, EndUserGivenName>>],
            middle_name[Option<HashMap<Option<LanguageTag>, EndUserMiddleName>>],
            nickname[Option<HashMap<Option<LanguageTag>, EndUserNickname>>],
            preferred_username[Option<EndUserUsername>],
            profile[Option<HashMap<Option<LanguageTag>, EndUserProfileUrl>>],
            picture[Option<HashMap<Option<LanguageTag>, EndUserPictureUrl>>],
            website[Option<HashMap<Option<LanguageTag>, EndUserWebsiteUrl>>],
            email[Option<EndUserEmail>],
            email_verified[Option<bool>],
            gender[Option<GC>],
            birthday[Option<EndUserBirthday>],
            zoneinfo[Option<EndUserTimezone>],
            locale[Option<LanguageTag>],
            phone_number[Option<EndUserPhoneNumber>],
            phone_number_verified[Option<bool>],
            address[Option<AddressClaim>],
            updated_at[Option<Seconds>],
        }
    ];
}
impl<AC, GC> AudiencesClaim for IdTokenClaims<AC, GC>
where AC: AdditionalClaims, GC: GenderClaim {
    fn audiences(&self) -> Option<&Vec<Audience>> {
        Some(IdTokenClaims::audiences(self))
    }
}
impl<'a, AC, GC> AudiencesClaim for &'a IdTokenClaims<AC, GC>
where AC: AdditionalClaims, GC: GenderClaim {
    fn audiences(&self) -> Option<&Vec<Audience>> {
        Some(IdTokenClaims::audiences(self))
    }
}
impl<AC, GC> IssuerClaim for IdTokenClaims<AC, GC>
where AC: AdditionalClaims, GC: GenderClaim {
    fn issuer(&self) -> Option<&IssuerUrl> {
        Some(IdTokenClaims::issuer(self))
    }
}
impl<'a, AC, GC> IssuerClaim for &'a IdTokenClaims<AC, GC>
where AC: AdditionalClaims, GC: GenderClaim {
    fn issuer(&self) -> Option<&IssuerUrl> {
        Some(IdTokenClaims::issuer(self))
    }
}

///
/// OpenID Connect authorization token.
///
/// The fields in this struct are defined in
/// [Section 3.1.3.3](http://openid.net/specs/openid-connect-core-1_0.html#TokenResponse).
///
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct IdTokenFields<AC, GC, JE, JS, JT>
where AC: AdditionalClaims,
        GC: GenderClaim,
        JE: JweContentEncryptionAlgorithm,
        JS: JwsSigningAlgorithm<JT>,
        JT: JsonWebKeyType {
    #[serde(with = "serde_id_token")]
    id_token: IdToken<AC, GC, JE, JS, JT>,
    #[serde(skip)]
    _phantom_jt: PhantomData<JT>,
}
impl<AC, GC, JE, JS, JT> IdTokenFields<AC, GC, JE, JS, JT>
where AC: AdditionalClaims,
        GC: GenderClaim,
        JE: JweContentEncryptionAlgorithm,
        JS: JwsSigningAlgorithm<JT>,
        JT: JsonWebKeyType {
    pub fn id_token(&self) -> &IdToken<AC, GC, JE, JS, JT> { &self.id_token }
    // FIXME: add extra_fields here to enable further extensibility by clients
}
impl<AC, GC, JE, JS, JT> ExtraTokenFields for IdTokenFields<AC, GC, JE, JS, JT>
where AC: AdditionalClaims,
        GC: GenderClaim,
        JE: JweContentEncryptionAlgorithm,
        JS: JwsSigningAlgorithm<JT>,
        JT: JsonWebKeyType {
}

pub trait JsonWebKey<JS, JT, JU> : Clone + Debug + DeserializeOwned + PartialEq + Serialize
where JS: JwsSigningAlgorithm<JT>, JT: JsonWebKeyType, JU: JsonWebKeyUse {
    fn key_id(&self) -> Option<&JsonWebKeyId>;
    fn key_type(&self) -> &JT;
    fn key_use(&self) -> Option<&JU>;
    fn new_symmetric(key: Vec<u8>) -> Self;
    fn verify_signature(
        &self,
        signature_alg: &JS,
        msg: &str,
        signature: &[u8]
    ) -> Result<(), SignatureVerificationError>;
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct JsonWebKeySet<JS, JT, JU, K>
where JS: JwsSigningAlgorithm<JT>,
        JT: JsonWebKeyType,
        JU: JsonWebKeyUse,
        K: JsonWebKey<JS, JT, JU> {
    // FIXME: write a test that ensures duplicate object member names cause an error
    // (see https://tools.ietf.org/html/rfc7517#section-5)
    // FIXME: add a deserializer that optionally ignores invalid keys rather than failing. That way,
    // clients can function using the keys that they do understand, which is fine if they only ever
    // get JWTs signed with those keys. See what other places we might want to be more tolerant of
    // deserialization errors.
    #[serde(bound = "K: JsonWebKey<JS, JT, JU>")]
    keys: Vec<K>,
    #[serde(skip)]
    _phantom_js: PhantomData<JS>,
    #[serde(skip)]
    _phantom_jt: PhantomData<JT>,
    #[serde(skip)]
    _phantom_ju: PhantomData<JU>,
}
impl<JS, JT, JU, K> JsonWebKeySet<JS, JT, JU, K>
where JS: JwsSigningAlgorithm<JT>,
        JT: JsonWebKeyType,
        JU: JsonWebKeyUse,
        K: JsonWebKey<JS, JT, JU> {
    pub fn keys(&self) -> &Vec<K> { &self.keys }
}

new_type![
    #[derive(Deserialize, Serialize)]
    JsonWebKeySetUrl(
        #[serde(
            deserialize_with = "deserialize_url",
            serialize_with = "serialize_url"
        )]
        Url
    )
    impl {
        // FIXME: don't depend on super::discovery in this module (factor this out into some kind
        // of HttpError?
        pub fn get_keys<JS, JT, JU, K>(
            &self
        ) -> Result<JsonWebKeySet<JS, JT, JU, K>, DiscoveryError>
        where JS: JwsSigningAlgorithm<JT>,
                JT: JsonWebKeyType,
                JU: JsonWebKeyUse,
                K: JsonWebKey<JS, JT, JU> {
            let key_response =
                HttpRequest {
                    url: &self.0,
                    method: HttpRequestMethod::Get,
                    headers: &vec![ACCEPT_JSON],
                    post_body: &vec![],
                }
                .request()
            .map_err(DiscoveryError::Request)?;

            // FIXME: improve error handling (i.e., is there a body response?)
            // possibly consolidate this error handling with discovery::get_provider_metadata().
            if key_response.status_code != HTTP_STATUS_OK {
                return Err(
                    DiscoveryError::Response(
                        key_response.status_code,
                        "unexpected HTTP status code".to_string()
                    )
                );
            }

            key_response
                .check_content_type(MIME_TYPE_JSON)
                .map_err(|err_msg| DiscoveryError::Response(key_response.status_code, err_msg))?;

            serde_json::from_slice(&key_response.body).map_err(DiscoveryError::Json)
        }
    }
];

// Public trait for accessing standard claims fields (via IdTokenClaims and UserInfoClaims).
pub trait StandardClaims<GC> where GC: GenderClaim {
    field_getter_decls![
        self {
            sub[SubjectIdentifier],
            name[Option<HashMap<Option<LanguageTag>, EndUserName>>],
            given_name[Option<HashMap<Option<LanguageTag>, EndUserGivenName>>],
            family_name[Option<HashMap<Option<LanguageTag>, EndUserGivenName>>],
            middle_name[Option<HashMap<Option<LanguageTag>, EndUserMiddleName>>],
            nickname[Option<HashMap<Option<LanguageTag>, EndUserNickname>>],
            preferred_username[Option<EndUserUsername>],
            profile[Option<HashMap<Option<LanguageTag>, EndUserProfileUrl>>],
            picture[Option<HashMap<Option<LanguageTag>, EndUserPictureUrl>>],
            website[Option<HashMap<Option<LanguageTag>, EndUserWebsiteUrl>>],
            email[Option<EndUserEmail>],
            email_verified[Option<bool>],
            gender[Option<GC>],
            birthday[Option<EndUserBirthday>],
            zoneinfo[Option<EndUserTimezone>],
            locale[Option<LanguageTag>],
            phone_number[Option<EndUserPhoneNumber>],
            phone_number_verified[Option<bool>],
            address[Option<AddressClaim>],
            updated_at[Option<Seconds>],
        }
    ];
}

// Private (fields accessed via IdTokenClaims and UserInfoClaims)
#[derive(Clone, Debug, PartialEq)]
struct StandardClaimsImpl<GC> where GC: GenderClaim {
    sub: SubjectIdentifier,
    name: Option<HashMap<Option<LanguageTag>, EndUserName>>,
    given_name: Option<HashMap<Option<LanguageTag>, EndUserGivenName>>,
    family_name: Option<HashMap<Option<LanguageTag>, EndUserGivenName>>,
    middle_name: Option<HashMap<Option<LanguageTag>, EndUserMiddleName>>,
    nickname: Option<HashMap<Option<LanguageTag>, EndUserNickname>>,
    preferred_username: Option<EndUserUsername>,
    profile: Option<HashMap<Option<LanguageTag>, EndUserProfileUrl>>,
    picture: Option<HashMap<Option<LanguageTag>, EndUserPictureUrl>>,
    website: Option<HashMap<Option<LanguageTag>, EndUserWebsiteUrl>>,
    email: Option<EndUserEmail>,
    email_verified: Option<bool>,
    gender: Option<GC>,
    birthday: Option<EndUserBirthday>,
    zoneinfo: Option<EndUserTimezone>,
    locale: Option<LanguageTag>,
    phone_number: Option<EndUserPhoneNumber>,
    phone_number_verified: Option<bool>,
    address: Option<AddressClaim>,
    updated_at: Option<Seconds>,
}
impl<'de, GC> Deserialize<'de> for StandardClaimsImpl<GC> where GC: GenderClaim {
    ///
    /// Special deserializer that supports [RFC 5646](https://tools.ietf.org/html/rfc5646) language
    /// tags associated with human-readable client metadata fields.
    ///
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error> where D: Deserializer<'de> {
        struct ClaimsVisitor<GC: GenderClaim>(PhantomData<GC>);
        impl<'de, GC> Visitor<'de> for ClaimsVisitor<GC> where GC: GenderClaim {
            type Value = StandardClaimsImpl<GC>;

            fn expecting(&self, formatter: &mut Formatter) -> FormatterResult {
                formatter.write_str("struct StandardClaimsImpl")
            }
            fn visit_map<V>(self, mut map: V) -> Result<Self::Value, V::Error>
            where V: MapAccess<'de> {
                deserialize_fields!{
                    map {
                        [sub]
                        [LanguageTag(name)]
                        [LanguageTag(given_name)]
                        [LanguageTag(family_name)]
                        [LanguageTag(middle_name)]
                        [LanguageTag(nickname)]
                        [Option(preferred_username)]
                        [LanguageTag(profile)]
                        [LanguageTag(picture)]
                        [LanguageTag(website)]
                        [Option(email)]
                        [Option(email_verified)]
                        [Option(gender)]
                        [Option(birthday)]
                        [Option(zoneinfo)]
                        [Option(locale)]
                        [Option(phone_number)]
                        [Option(phone_number_verified)]
                        [Option(address)]
                        [Option(updated_at)]
                    }
                }
            }
        }
        deserializer
            .deserialize_map(
                ClaimsVisitor(PhantomData)
            )
    }
}
impl<GC> Serialize for StandardClaimsImpl<GC> where GC: GenderClaim {
    #[allow(cyclomatic_complexity)]
    fn serialize<SE>(&self, serializer: SE) -> Result<SE::Ok, SE::Error> where SE: Serializer {
        serialize_fields!{
            self -> serializer {
                [sub]
                [LanguageTag(name)]
                [LanguageTag(given_name)]
                [LanguageTag(family_name)]
                [LanguageTag(middle_name)]
                [LanguageTag(nickname)]
                [Option(preferred_username)]
                [LanguageTag(profile)]
                [LanguageTag(picture)]
                [LanguageTag(website)]
                [Option(email)]
                [Option(email_verified)]
                [Option(gender)]
                [Option(birthday)]
                [Option(zoneinfo)]
                [Option(locale)]
                [Option(phone_number)]
                [Option(phone_number_verified)]
                [Option(address)]
                [Option(updated_at)]
            }
        }
    }
}

fn join_optional_vec<T>(vec_opt: Option<&Vec<T>>) -> Option<String> where T: AsRef<str> {
    match vec_opt {
        Some(entries) => Some(
            entries
                .iter()
                .map(|entries| entries.as_ref())
                .collect::<Vec<_>>()
                .join(" ")
        ),
        None => None,
    }
}

#[derive(Debug, Fail)]
pub enum UserInfoError {
    #[fail(display = "Failed to verify claims")]
    ClaimsVerification(#[cause] ClaimsVerificationError),
    #[fail(display = "Request failed")]
    Request(#[cause] curl::Error),
    #[fail(display = "Response error (status={}): {}", _0, _1)]
    Response(u32, String),
    #[fail(display = "Failed to parse response")]
    Json(#[cause] serde_json::Error),
    #[fail(display = "Other error: {}", _0)]
    Other(String),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
enum UnverifiedUserInfoClaims<AC, GC, JE, JS, JT>
where AC: AdditionalClaims,
        GC: GenderClaim,
        JE: JweContentEncryptionAlgorithm,
        JS: JwsSigningAlgorithm<JT>,
        JT: JsonWebKeyType {
    JsonClaims(
        #[serde(bound = "AC: AdditionalClaims")]
        UserInfoClaims<AC, GC>
    ),
    JwtClaims(
        #[serde(bound = "AC: AdditionalClaims")]
        JsonWebToken<UserInfoClaims<AC, GC>, JE, JS, JT>
    )
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct UserInfoClaims<AC, GC> where AC: AdditionalClaims, GC: GenderClaim {
    iss: Option<IssuerUrl>,
    // FIXME: this needs to be a vector, but it may also come as a single string
    aud: Option<Vec<Audience>>,

    #[serde(bound = "GC: GenderClaim")]
    #[serde(flatten)]
    standard_claims: StandardClaimsImpl<GC>,

    #[serde(bound = "AC: AdditionalClaims")]
    #[serde(flatten)]
    additional_claims: AC
}
// FIXME: see what other structs should have friendlier trait interfaces like this one
impl<AC, GC> UserInfoClaims<AC, GC>
where AC: AdditionalClaims, GC: GenderClaim {
    pub fn issuer(&self) -> Option<&IssuerUrl> { self.iss.as_ref() }
    pub fn audiences(&self) -> Option<&Vec<Audience>> { self.aud.as_ref() }
    pub fn additional_claims(&self) -> &AC { &self.additional_claims }
}
impl<AC, GC> StandardClaims<GC> for UserInfoClaims<AC, GC>
where AC: AdditionalClaims, GC: GenderClaim {
    field_getters![
        self [self.standard_claims] {
            sub[SubjectIdentifier],
            name[Option<HashMap<Option<LanguageTag>, EndUserName>>],
            given_name[Option<HashMap<Option<LanguageTag>, EndUserGivenName>>],
            family_name[Option<HashMap<Option<LanguageTag>, EndUserGivenName>>],
            middle_name[Option<HashMap<Option<LanguageTag>, EndUserMiddleName>>],
            nickname[Option<HashMap<Option<LanguageTag>, EndUserNickname>>],
            preferred_username[Option<EndUserUsername>],
            profile[Option<HashMap<Option<LanguageTag>, EndUserProfileUrl>>],
            picture[Option<HashMap<Option<LanguageTag>, EndUserPictureUrl>>],
            website[Option<HashMap<Option<LanguageTag>, EndUserWebsiteUrl>>],
            email[Option<EndUserEmail>],
            email_verified[Option<bool>],
            gender[Option<GC>],
            birthday[Option<EndUserBirthday>],
            zoneinfo[Option<EndUserTimezone>],
            locale[Option<LanguageTag>],
            phone_number[Option<EndUserPhoneNumber>],
            phone_number_verified[Option<bool>],
            address[Option<AddressClaim>],
            updated_at[Option<Seconds>],
        }
    ];
}

impl<AC, GC> AudiencesClaim for UserInfoClaims<AC, GC>
    where AC: AdditionalClaims, GC: GenderClaim {
    fn audiences(&self) -> Option<&Vec<Audience>> {
        UserInfoClaims::audiences(&self)
    }
}
impl<'a, AC, GC> AudiencesClaim for &'a UserInfoClaims<AC, GC>
    where AC: AdditionalClaims, GC: GenderClaim {
    fn audiences(&self) -> Option<&Vec<Audience>> {
        UserInfoClaims::audiences(&self)
    }
}

impl<AC, GC> IssuerClaim for UserInfoClaims<AC, GC>
    where AC: AdditionalClaims, GC: GenderClaim {
    fn issuer(&self) -> Option<&IssuerUrl> {
        UserInfoClaims::issuer(&self)
    }
}
impl<'a, AC, GC> IssuerClaim for &'a UserInfoClaims<AC, GC>
    where AC: AdditionalClaims, GC: GenderClaim {
    fn issuer(&self) -> Option<&IssuerUrl> {
        UserInfoClaims::issuer(&self)
    }
}

new_type![
    #[derive(Deserialize, Serialize)]
    UserInfoUrl(
        #[serde(
            deserialize_with = "deserialize_url",
            serialize_with = "serialize_url"
        )]
        Url
    )
    impl {
        pub fn get_user_info<AC, GC, JE, JS, JT, JU, K>(
            &self,
            access_token: &AccessToken,
            verifier: &UserInfoVerifier<JE, JS, JT, JU, K>,
        ) -> Result<UserInfoClaims<AC, GC>, UserInfoError>
        where AC: AdditionalClaims,
                GC: GenderClaim,
                JE: JweContentEncryptionAlgorithm,
                JS: JwsSigningAlgorithm<JT>,
                JT: JsonWebKeyType,
                JU: JsonWebKeyUse,
                K: JsonWebKey<JS, JT, JU>{
            let (auth_header, auth_value) = auth_bearer(access_token);
            let user_info_response =
                HttpRequest {
                    url: &self.0,
                    method: HttpRequestMethod::Get,
                    headers: &vec![ACCEPT_JSON, (auth_header, auth_value.as_ref())],
                    post_body: &vec![],
                }
                .request()
                .map_err(UserInfoError::Request)?;

            // FIXME: improve error handling (i.e., is there a body response?)
            // possibly consolidate this error handling with discovery::get_provider_metadata().
            if user_info_response.status_code != HTTP_STATUS_OK {
                return Err(
                    UserInfoError::Response(
                        user_info_response.status_code,
                        "unexpected HTTP status code".to_string()
                    )
                );
            }

            match user_info_response.content_type.as_ref().map(String::as_str) {
                None | Some(MIME_TYPE_JSON) => {
                    verifier
                        .verified_claims(
                            UnverifiedUserInfoClaims::JsonClaims(
                                serde_json::from_slice(&user_info_response.body)
                                    .map_err(UserInfoError::Json)?
                            )
                        )
                        .map_err(UserInfoError::ClaimsVerification)
                }
                Some(MIME_TYPE_JWT) => {
                    let jwt_str =
                        str::from_utf8(&user_info_response.body)
                            .map_err(|_|
                                UserInfoError::Other(
                                    "response body has invalid UTF-8 encoding".to_string()
                                )
                            )?;
                    // TODO: Implement a simple deserializer so that we can go straight from a str
                    // to a JsonWebToken without first converting to/from JSON.
                    let jwt_json =
                        serde_json::to_string(&jwt_str)
                            .map_err(UserInfoError::Json)?;
                    verifier
                        .verified_claims(
                            UnverifiedUserInfoClaims::JwtClaims(
                                serde_json::from_str(&jwt_json)
                                    .map_err(UserInfoError::Json)?
                            )
                        )
                        .map_err(UserInfoError::ClaimsVerification)
                }
                Some(content_type) =>
                    Err(
                        UserInfoError::Response(
                            user_info_response.status_code,
                            format!("unexpected response Content-Type: `{}`", content_type)
                        )
                    ),
            }
        }
    }
];
