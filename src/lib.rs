#![warn(missing_docs)]
#![cfg_attr(feature = "nightly", feature(type_alias_enum_variants))]
//!
//! [OpenID Connect](https://openid.net/specs/openid-connect-core-1_0.html) library.
//!
//! This library provides extensible, strongly-typed interfaces for the OpenID Connect protocol.
//! For convenience, the [`core`] module provides type aliases for common usage that adheres to the
//! [OpenID Connect Core](https://openid.net/specs/openid-connect-core-1_0.html) spec. Users of
//! this crate may define their own extensions and custom type parameters in lieu of using the
//! [`core`] module.
//!
//! # OpenID Connect Relying Party (Client) Interface
//!
//! The [`Client`] struct provides the OpenID Connect Relying Party interface. The most common
//! usage is provided by the [`core::CoreClient`] type alias.
//!
//! ## Examples
//!
//! * [Google](https://github.com/ramosbugs/openidconnect-rs/tree/master/examples/google.rs)
//!
//! ## Getting started: Authorization Code Grant w/ PKCE
//!
//! This is the most common OIDC/OAuth2 flow. PKCE is recommended whenever the client has no
//! client secret or has a client secret that cannot remain confidential (e.g., native, mobile, or
//! client-side web applications).
//!
//! ### Example
//!
//! ```
//! extern crate base64;
//! extern crate openidconnect;
//! extern crate url;
//!
//! use openidconnect::{
//!     AccessTokenHash,
//!     AuthenticationFlow,
//!     AuthorizationCode,
//!     ClientId,
//!     ClientSecret,
//!     CsrfToken,
//!     Nonce,
//!     IssuerUrl,
//!     PkceCodeChallenge,
//!     RedirectUrl,
//!     Scope,
//! };
//! use openidconnect::core::{CoreClient, CoreProviderMetadata, CoreResponseType};
//! use openidconnect::reqwest::http_client;
//! use url::Url;
//!
//! # extern crate failure;
//! # fn err_wrapper() -> Result<(), failure::Error> {
//! // Use OpenID Connect Discovery to fetch the provider metadata.
//! use openidconnect::{OAuth2TokenResponse, TokenResponse};
//! let provider_metadata = CoreProviderMetadata::discover(
//!     &IssuerUrl::new("https://accounts.example.com".to_string())?,
//!     http_client,
//! )?;
//!
//! // Create an OpenID Connect client by specifying the client ID, client secret, authorization URL
//! // and token URL.
//! let client =
//!     CoreClient::from_provider_metadata(
//!         provider_metadata,
//!         ClientId::new("client_id".to_string()),
//!         Some(ClientSecret::new("client_secret".to_string())),
//!     )
//!     // Set the URL the user will be redirected to after the authorization process.
//!     .set_redirect_uri(RedirectUrl::new("http://redirect".to_string())?);
//!
//! // Generate a PKCE challenge.
//! let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
//!
//! // Generate the full authorization URL.
//! let (auth_url, csrf_token, nonce) = client
//!     .authorize_url(
//!         // If using nightly Rust, a CoreAuthenticationFlow trait alias is available by
//!         // enabling the "nightly" feature in Cargo.toml.
//!         AuthenticationFlow::<CoreResponseType>::AuthorizationCode,
//!         CsrfToken::new_random,
//!         Nonce::new_random,
//!     )
//!     // Set the desired scopes.
//!     .add_scope(Scope::new("read".to_string()))
//!     .add_scope(Scope::new("write".to_string()))
//!     // Set the PKCE code challenge.
//!     .set_pkce_challenge(pkce_challenge)
//!     .url();
//!
//! // This is the URL you should redirect the user to, in order to trigger the authorization
//! // process.
//! println!("Browse to: {}", auth_url);
//!
//! // Once the user has been redirected to the redirect URL, you'll have access to the
//! // authorization code. For security reasons, your code should verify that the `state`
//! // parameter returned by the server matches `csrf_state`.
//!
//! // Now you can exchange it for an access token and ID token.
//! let token_response =
//!     client
//!         .exchange_code(AuthorizationCode::new("some authorization code".to_string()))
//!         // Set the PKCE code verifier.
//!         .set_pkce_verifier(pkce_verifier)
//!         .request(http_client)?;
//!
//! // Extract the ID token claims after verifying its authenticity and nonce.
//! let id_token = token_response.id_token().claims(&client.id_token_verifier(), &nonce)?;
//!
//! // Verify the access token hash to ensure that the access token hasn't been substituted for
//! // another user's.
//! if let Some(expected_access_token_hash) = id_token.access_token_hash() {
//!     let actual_access_token_hash = AccessTokenHash::from_token(
//!         token_response.access_token(),
//!         &token_response.id_token().signing_alg()?
//!     )?;
//!     if actual_access_token_hash != *expected_access_token_hash {
//!         return Err(failure::Error::from_boxed_compat("Invalid access token".into()));
//!     }
//! }
//!
//! // The authenticated user's identity is now available. See the IdTokenClaims struct for a
//! // complete listing of the available claims.
//! println!(
//!     "User {} with e-mail address {} has authenticated successfully",
//!     id_token.subject().as_str(),
//!     id_token.email().map(|email| email.as_str()).unwrap_or("<not provided>"),
//! );
//!
//! // See the OAuth2TokenResponse trait for a listing of other available fields such as
//! // access_token() and refresh_token().
//!
//! # Ok(())
//! # }
//! # fn main() {}
//! ```
//!
//! # OpenID Connect Provider (Server) Interface
//!
//! This library does not implement a complete OpenID Connect Provider, which requires
//! functionality such as credential and session management. However, it does provide
//! strongly-typed interfaces for parsing and building OpenID Connect protocol messages.
//!
//! ## OpenID Connect Discovery document
//!
//! The [`ProviderMetadata`] struct implements the
//! [OpenID Connect Discovery document](https://openid.net/specs/openid-connect-discovery-1_0.html#ProviderConfig).
//! This data structure should be serialized to JSON and served via the
//! `GET .well-known/openid-configuration` path relative to your provider's issuer URL.
//!
//! ### Example
//!
//! ```
//! extern crate openidconnect;
//! extern crate serde_json;
//! extern crate url;
//!
//! use openidconnect::{
//!     AuthUrl,
//!     EmptyAdditionalProviderMetadata,
//!     IssuerUrl,
//!     JsonWebKeySetUrl,
//!     ResponseTypes,
//!     Scope,
//!     TokenUrl,
//!     UserInfoUrl,
//! };
//! use openidconnect::core::{
//!     CoreClaimName,
//!     CoreJwsSigningAlgorithm,
//!     CoreProviderMetadata,
//!     CoreResponseType,
//!     CoreSubjectIdentifierType
//! };
//! use url::Url;
//!
//! # extern crate failure;
//! # fn err_wrapper() -> Result<String, failure::Error> {
//! let provider_metadata = CoreProviderMetadata::new(
//!     // Parameters required by the OpenID Connect Discovery spec.
//!     IssuerUrl::new("https://accounts.example.com".to_string())?,
//!     AuthUrl::new("https://accounts.example.com/authorize".to_string())?,
//!     // Use the JsonWebKeySet struct to serve the JWK Set at this URL.
//!     JsonWebKeySetUrl::new("https://accounts.example.com/jwk".to_string())?,
//!     // Supported response types (flows).
//!     vec![
//!         // Recommended: support the code flow.
//!         ResponseTypes::new(vec![CoreResponseType::Code]),
//!         // Optional: support the implicit flow.
//!         ResponseTypes::new(vec![CoreResponseType::Token, CoreResponseType::IdToken])
//!         // Other flows including hybrid flows may also be specified here.
//!     ],
//!     // For user privacy, the Pairwise subject identifier type is preferred. This prevents
//!     // distinct relying parties (clients) from knowing whether their users represent the same
//!     // real identities. This identifier type is only useful for relying parties that don't
//!     // receive the 'email', 'profile' or other personally-identifying scopes.
//!     // The Public subject identifier type is also supported.
//!     vec![CoreSubjectIdentifierType::Pairwise],
//!     // Support the RS256 signature algorithm.
//!     vec![CoreJwsSigningAlgorithm::RsaSsaPssSha256],
//!     // OpenID Connect Providers may supply custom metadata by providing a struct that
//!     // implements the AdditionalProviderMetadata trait. This requires manually using the
//!     // generic ProviderMetadata struct rather than the CoreProviderMetadata type alias,
//!     // however.
//!     EmptyAdditionalProviderMetadata {},
//! )
//! // Specify the token endpoint (required for the code flow).
//! .set_token_endpoint(Some(TokenUrl::new("https://accounts.example.com/token".to_string())?))
//! // Recommended: support the UserInfo endpoint.
//! .set_userinfo_endpoint(
//!     Some(UserInfoUrl::new("https://accounts.example.com/userinfo".to_string())?)
//! )
//! // Recommended: specify the supported scopes.
//! .set_scopes_supported(Some(vec![
//!     Scope::new("openid".to_string()),
//!     Scope::new("email".to_string()),
//!     Scope::new("profile".to_string()),
//! ]))
//! // Recommended: specify the supported ID token claims.
//! .set_claims_supported(Some(vec![
//!     // Providers may also define an enum instead of using CoreClaimName.
//!     CoreClaimName::new("sub".to_string()),
//!     CoreClaimName::new("aud".to_string()),
//!     CoreClaimName::new("email".to_string()),
//!     CoreClaimName::new("email_verified".to_string()),
//!     CoreClaimName::new("exp".to_string()),
//!     CoreClaimName::new("iat".to_string()),
//!     CoreClaimName::new("iss".to_string()),
//!     CoreClaimName::new("name".to_string()),
//!     CoreClaimName::new("given_name".to_string()),
//!     CoreClaimName::new("family_name".to_string()),
//!     CoreClaimName::new("picture".to_string()),
//!     CoreClaimName::new("locale".to_string()),
//! ]));
//!
//! serde_json::to_string(&provider_metadata).map_err(failure::Error::from)
//! # }
//! # fn main() {}
//! ```
//!
//! ## OpenID Connect Discovery JSON Web Key Set
//!
//! The JSON Web Key Set (JWKS) provides the public keys that relying parties (clients) use to
//! verify the authenticity of ID tokens returned by this OpenID Connect Provider. The
//! [`JsonWebKeySet`] data structure should be serialized as JSON and served at the URL specified
//! in the `jwks_uri` field of the [`ProviderMetadata`] returned in the OpenID Connect Discovery
//! document.
//!
//! ### Example
//!
//! ```
//! use openidconnect::{JsonWebKeyId, PrivateSigningKey};
//! use openidconnect::core::{CoreJsonWebKey, CoreJsonWebKeySet, CoreRsaPrivateSigningKey};
//!
//! # extern crate failure;
//! # fn err_wrapper() -> Result<String, failure::Error> {
//! # let rsa_pem = "";
//! let jwks = CoreJsonWebKeySet::new(
//!     vec![
//!         // RSA keys may also be constructed directly using CoreJsonWebKey::new_rsa(). Providers
//!         // aiming to support other key types may provide their own implementation of the
//!         // JsonWebKey trait or submit a PR to add the desired support to this crate.
//!         CoreRsaPrivateSigningKey::from_pem(
//!             &rsa_pem,
//!             Some(JsonWebKeyId::new("key1".to_string()))
//!         )
//!         .expect("Invalid RSA private key")
//!         .as_verification_key()
//!     ]
//! );
//!
//! serde_json::to_string(&jwks).map_err(failure::Error::from)
//! # }
//! # fn main() {}
//! ```
//!
//! ## OpenID Connect ID Token
//!
//! The [`IdToken::new`] method is used for signing ID token claims, which can then be returned
//! from the token endpoint as part of the [`StandardTokenResponse`] struct
//! (or [`core::CoreTokenResponse`] type alias). The ID token can also be serialized to a string
//! using the `IdToken::to_string` method and returned directly from the authorization endpoint
//! when the implicit flow or certain hybrid flows are used. Note that in these flows, ID tokens
//! must only be returned in the URL fragment, and never as a query parameter.
//!
//! The ID token contains a combination of the
//! [OpenID Connect Standard Claims](https://openid.net/specs/openid-connect-core-1_0.html#StandardClaims)
//! (see [`StandardClaims`]) and claims specific to the
//! [OpenID Connect ID Token](https://openid.net/specs/openid-connect-core-1_0.html#IDToken)
//! (see [`IdTokenClaims`]).
//!
//! ### Example
//!
//! ```
//! extern crate chrono;
//! extern crate openidconnect;
//!
//! use chrono::{Duration, Utc};
//! use openidconnect::{
//!     AccessToken,
//!     Audience,
//!     EmptyAdditionalClaims,
//!     EmptyExtraTokenFields,
//!     EndUserEmail,
//!     IssuerUrl,
//!     JsonWebKeyId,
//!     StandardClaims,
//!     SubjectIdentifier,
//! };
//! use openidconnect::core::{
//!     CoreIdToken,
//!     CoreIdTokenClaims,
//!     CoreIdTokenFields,
//!     CoreJwsSigningAlgorithm,
//!     CoreRsaPrivateSigningKey,
//!     CoreTokenResponse,
//!     CoreTokenType,
//! };
//!
//! # extern crate failure;
//! # fn err_wrapper() -> Result<CoreTokenResponse, failure::Error> {
//! # let rsa_pem = "";
//! # let access_token = AccessToken::new("".to_string());
//! let id_token = CoreIdToken::new(
//!     CoreIdTokenClaims::new(
//!         // Specify the issuer URL for the OpenID Connect Provider.
//!         IssuerUrl::new("https://accounts.example.com".to_string())?,
//!         // The audience is usually a single entry with the client ID of the client for whom
//!         // the ID token is intended. This is a required claim.
//!         vec![Audience::new("client-id-123".to_string())],
//!         // The ID token expiration is usually much shorter than that of the access or refresh
//!         // tokens issued to clients.
//!         Utc::now() + Duration::seconds(300),
//!         // The issue time is usually the current time.
//!         Utc::now(),
//!         // Set the standard claims defined by the OpenID Connect Core spec.
//!         StandardClaims::new(
//!             // Stable subject identifiers are recommended in place of e-mail addresses or other
//!             // potentially unstable identifiers. This is the only required claim.
//!             SubjectIdentifier::new("5f83e0ca-2b8e-4e8c-ba0a-f80fe9bc3632".to_string())
//!         )
//!         // Optional: specify the user's e-mail address. This should only be provided if the
//!         // client has been granted the 'profile' or 'email' scopes.
//!         .set_email(Some(EndUserEmail::new("bob@example.com".to_string())))
//!         // Optional: specify whether the provider has verified the user's e-mail address.
//!         .set_email_verified(Some(true)),
//!         // OpenID Connect Providers may supply custom claims by providing a struct that
//!         // implements the AdditionalClaims trait. This requires manually using the
//!         // generic IdTokenClaims struct rather than the CoreIdTokenClaims type alias,
//!         // however.
//!         EmptyAdditionalClaims {},
//!     ),
//!     // The private key used for signing the ID token. For confidential clients (those able
//!     // to maintain a client secret), a CoreHmacKey can also be used, in conjunction
//!     // with one of the CoreJwsSigningAlgorithm::HmacSha* signing algorithms. When using an
//!     // HMAC-based signing algorithm, the UTF-8 representation of the client secret should
//!     // be used as the HMAC key.
//!     &CoreRsaPrivateSigningKey::from_pem(
//!             &rsa_pem,
//!             Some(JsonWebKeyId::new("key1".to_string()))
//!         )
//!         .expect("Invalid RSA private key"),
//!     // Uses the RS256 signature algorithm. This crate supports any RS*, PS*, or HS*
//!     // signature algorithm.
//!     CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha256,
//!     // When returning the ID token alongside an access token (e.g., in the Authorization Code
//!     // flow), it is recommended to pass the access token here to set the `at_hash` claim
//!     // automatically.
//!     Some(&access_token),
//!     // When returning the ID token alongside an authorization code (e.g., in the implicit
//!     // flow), it is recommended to pass the authorization code here to set the `c_hash` claim
//!     // automatically.
//!     None,
//! )?;
//!
//! Ok(CoreTokenResponse::new(
//!     AccessToken::new("some_secret".to_string()),
//!     CoreTokenType::Bearer,
//!     CoreIdTokenFields::new(id_token, EmptyExtraTokenFields {}),
//! ))
//! # }
//! # fn main() {}
//! ```

extern crate base64;
extern crate chrono;
extern crate failure;
extern crate futures;
#[macro_use]
extern crate failure_derive;
extern crate http as http_;
extern crate itertools;
extern crate oauth2;
extern crate rand;
extern crate ring;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate untrusted;
extern crate url;

#[cfg(test)]
extern crate color_backtrace;

#[cfg(test)]
#[macro_use]
extern crate pretty_assertions;

use std::borrow::Cow;
use std::marker::PhantomData;
use std::str;
use std::time::Duration;

use oauth2::helpers::variant_name;
use oauth2::ResponseType as OAuth2ResponseType;

#[cfg(feature = "curl")]
pub use oauth2::curl;

#[cfg(feature = "reqwest")]
pub use oauth2::reqwest;

pub use oauth2::{
    AccessToken, AuthType, AuthUrl, AuthorizationCode, ClientId, ClientSecret, CodeTokenRequest,
    CsrfToken, EmptyExtraTokenFields, ErrorResponse, ErrorResponseType, ExtraTokenFields,
    HttpRequest, HttpResponse, PkceCodeChallenge, PkceCodeChallengeMethod, PkceCodeVerifier,
    RedirectUrl, RefreshToken, RefreshTokenRequest, RequestTokenError, Scope,
    StandardErrorResponse, StandardTokenResponse, TokenResponse as OAuth2TokenResponse, TokenType,
    TokenUrl,
};
use url::Url;

pub use claims::{
    AdditionalClaims, AddressClaim, EmptyAdditionalClaims, GenderClaim, StandardClaims,
};
pub use discovery::{
    AdditionalProviderMetadata, DiscoveryError, EmptyAdditionalProviderMetadata, ProviderMetadata,
};
pub use id_token::{IdToken, IdTokenClaims};
pub use id_token::{IdTokenFields, RefreshIdTokenFields};
pub use jwt::JsonWebTokenError;
use jwt::{JsonWebToken, JsonWebTokenAccess, JsonWebTokenAlgorithm, JsonWebTokenHeader};
// Flatten the module hierarchy involving types. They're only separated to improve code
// organization.
pub use types::{
    AccessTokenHash, AddressCountry, AddressLocality, AddressPostalCode, AddressRegion,
    ApplicationType, Audience, AuthDisplay, AuthPrompt, AuthenticationContextClass,
    AuthenticationMethodReference, AuthorizationCodeHash, ClaimName, ClaimType, ClientAuthMethod,
    ClientConfigUrl, ClientContactEmail, ClientName, ClientUrl, EndUserBirthday, EndUserEmail,
    EndUserFamilyName, EndUserGivenName, EndUserMiddleName, EndUserName, EndUserNickname,
    EndUserPhoneNumber, EndUserPictureUrl, EndUserProfileUrl, EndUserTimezone, EndUserUsername,
    EndUserWebsiteUrl, FormattedAddress, GrantType, InitiateLoginUrl, IssuerUrl, JsonWebKey,
    JsonWebKeyId, JsonWebKeySet, JsonWebKeySetUrl, JsonWebKeyType, JsonWebKeyUse,
    JweContentEncryptionAlgorithm, JweKeyManagementAlgorithm, JwsSigningAlgorithm, LanguageTag,
    LocalizedClaim, LoginHint, LogoUrl, Nonce, OpPolicyUrl, OpTosUrl, PolicyUrl, PrivateSigningKey,
    RegistrationAccessToken, RegistrationUrl, RequestUrl, ResponseMode, ResponseType,
    ResponseTypes, SectorIdentifierUrl, ServiceDocUrl, SigningError, StreetAddress,
    SubjectIdentifier, SubjectIdentifierType, ToSUrl,
};
pub use user_info::{
    NoUserInfoEndpoint, UserInfoClaims, UserInfoError, UserInfoJsonWebToken, UserInfoRequest,
    UserInfoUrl,
};
use verification::{AudiencesClaim, IssuerClaim};
pub use verification::{
    ClaimsVerificationError, IdTokenVerifier, NonceVerifier, SignatureVerificationError,
    UserInfoVerifier,
};

// Defined first since other modules need the macros, and definition order is significant for
// macros. This module is private.
#[macro_use]
mod macros;

/// Baseline OpenID Connect implementation and types.
pub mod core;

/// OpenID Connect Dynamic Client Registration.
pub mod registration;

// Private modules since we may move types between different modules; these are exported publicly
// via the pub use above.
mod claims;
mod discovery;
mod id_token;
pub(crate) mod types;
mod user_info;
mod verification;

// Private module for HTTP(S) utilities.
mod http;

// Private module for JWT utilities.
mod jwt;

const CONFIG_URL_SUFFIX: &str = ".well-known/openid-configuration";
const OPENID_SCOPE: &str = "openid";

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
    /// call `Client::exchange_code()` with the authorization code in order to retrieve an
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
    ///     https://openid.net/specs/oauth-v2-multiple-response-types-1_0.html). The enum value
    /// contains the desired `response_type`s. See
    /// [Section 3](https://openid.net/specs/openid-connect-core-1_0.html#Authentication) for
    /// details.
    ///
    Hybrid(Vec<RT>),
}

/// OpenID Connect client.
#[derive(Clone, Debug)]
pub struct Client<AC, AD, GC, JE, JS, JT, JU, K, P, RR, TE, TR, TT>
where
    AC: AdditionalClaims,
    AD: AuthDisplay,
    GC: GenderClaim,
    JE: JweContentEncryptionAlgorithm<JT>,
    JS: JwsSigningAlgorithm<JT>,
    JT: JsonWebKeyType,
    JU: JsonWebKeyUse,
    K: JsonWebKey<JS, JT, JU>,
    P: AuthPrompt,
    RR: RefreshTokenResponse<AC, GC, JE, JS, JT, TT>,
    TE: ErrorResponse,
    TR: TokenResponse<AC, GC, JE, JS, JT, TT>,
    TT: TokenType + 'static,
{
    oauth2_client: oauth2::Client<TE, TR, TT>,
    // We need a separate client for refresh tokens because the ID token is optional in the
    // refresh response, and the OAuth2 client returns the same data type for refresh requests as
    // for exchange_code requests.
    refresh_oauth2_client: oauth2::Client<TE, RR, TT>,
    client_id: ClientId,
    client_secret: Option<ClientSecret>,
    issuer: IssuerUrl,
    userinfo_endpoint: Option<UserInfoUrl>,
    jwks: JsonWebKeySet<JS, JT, JU, K>,
    _phantom: PhantomData<(AC, AD, GC, JE, P)>,
}
impl<AC, AD, GC, JE, JS, JT, JU, K, P, RR, TE, TR, TT>
    Client<AC, AD, GC, JE, JS, JT, JU, K, P, RR, TE, TR, TT>
where
    AC: AdditionalClaims,
    AD: AuthDisplay,
    GC: GenderClaim,
    JE: JweContentEncryptionAlgorithm<JT>,
    JS: JwsSigningAlgorithm<JT>,
    JT: JsonWebKeyType,
    JU: JsonWebKeyUse,
    K: JsonWebKey<JS, JT, JU>,
    P: AuthPrompt,
    RR: RefreshTokenResponse<AC, GC, JE, JS, JT, TT>,
    TE: ErrorResponse,
    TR: TokenResponse<AC, GC, JE, JS, JT, TT>,
    TT: TokenType + 'static,
{
    ///
    /// Initializes an OpenID Connect client.
    ///
    pub fn new(
        client_id: ClientId,
        client_secret: Option<ClientSecret>,
        issuer: IssuerUrl,
        auth_url: AuthUrl,
        token_url: Option<TokenUrl>,
        userinfo_endpoint: Option<UserInfoUrl>,
        jwks: JsonWebKeySet<JS, JT, JU, K>,
    ) -> Self {
        Client {
            oauth2_client: oauth2::Client::new(
                client_id.clone(),
                client_secret.clone(),
                auth_url.clone(),
                token_url.clone(),
            ),
            refresh_oauth2_client: oauth2::Client::new(
                client_id.clone(),
                client_secret.clone(),
                auth_url,
                token_url,
            ),
            client_id,
            client_secret,
            issuer,
            userinfo_endpoint,
            jwks,
            _phantom: PhantomData,
        }
    }

    ///
    /// Initializes an OpenID Connect client from OpenID Connect Discovery provider metadata.
    ///
    /// Use [`ProviderMetadata::discover`] or
    /// [`ProviderMetadata::discover_async`] to fetch the provider metadata.
    ///
    pub fn from_provider_metadata<A, CA, CN, CT, G, JK, RM, RT, S>(
        provider_metadata: ProviderMetadata<A, AD, CA, CN, CT, G, JE, JK, JS, JT, JU, K, RM, RT, S>,
        client_id: ClientId,
        client_secret: Option<ClientSecret>,
    ) -> Self
    where
        A: AdditionalProviderMetadata,
        CA: ClientAuthMethod,
        CN: ClaimName,
        CT: ClaimType,
        G: GrantType,
        JK: JweKeyManagementAlgorithm,
        RM: ResponseMode,
        RT: ResponseType,
        S: SubjectIdentifierType,
    {
        Self::new(
            client_id,
            client_secret,
            provider_metadata.issuer().clone(),
            provider_metadata.authorization_endpoint().clone(),
            provider_metadata.token_endpoint().cloned(),
            provider_metadata.userinfo_endpoint().cloned(),
            provider_metadata.jwks().to_owned(),
        )
    }

    ///
    /// Configures the type of client authentication used for communicating with the authorization
    /// server.
    ///
    /// The default is to use HTTP Basic authentication, as recommended in
    /// [Section 2.3.1 of RFC 6749](https://tools.ietf.org/html/rfc6749#section-2.3.1).
    ///
    pub fn set_auth_type(mut self, auth_type: AuthType) -> Self {
        self.oauth2_client = self.oauth2_client.set_auth_type(auth_type.clone());
        self.refresh_oauth2_client = self.refresh_oauth2_client.set_auth_type(auth_type);
        self
    }

    ///
    /// Sets the the redirect URL used by the authorization endpoint.
    ///
    pub fn set_redirect_uri(mut self, redirect_uri: RedirectUrl) -> Self {
        self.oauth2_client = self.oauth2_client.set_redirect_url(redirect_uri.clone());
        self.refresh_oauth2_client = self.refresh_oauth2_client.set_redirect_url(redirect_uri);
        self
    }

    ///
    /// Returns an ID token verifier for use with the [`IdToken::claims`] method.
    ///
    pub fn id_token_verifier(&self) -> IdTokenVerifier<JS, JT, JU, K> {
        if let Some(ref client_secret) = self.client_secret {
            IdTokenVerifier::new_confidential_client(
                self.client_id.clone(),
                client_secret.clone(),
                self.issuer.clone(),
                self.jwks.clone(),
            )
        } else {
            IdTokenVerifier::new_public_client(
                self.client_id.clone(),
                self.issuer.clone(),
                self.jwks.clone(),
            )
        }
    }

    ///
    /// Generates an authorization URL for a new authorization request.
    ///
    /// NOTE: [Passing authorization request parameters as a JSON Web Token
    /// ](https://openid.net/specs/openid-connect-core-1_0.html#JWTRequests)
    /// instead of URL query parameters is not currently supported. The
    /// [`claims` parameter](https://openid.net/specs/openid-connect-core-1_0.html#ClaimsParameter)
    /// is also not directly supported, although the [`AuthorizationRequest::add_extra_param`]
    /// method can be used to add custom parameters, including `claims`.
    ///
    /// # Arguments
    ///
    /// * `authentication_flow` - The authentication flow to use (code, implicit, or hybrid).
    /// * `state_fn` - A function that returns an opaque value used by the client to maintain state
    ///   between the request and callback. The authorization server includes this value when
    ///   redirecting the user-agent back to the client.
    /// * `nonce_fn` - Similar to `state_fn`, but used to generate an opaque nonce to be used
    ///   when verifying the ID token returned by the OpenID Connect Provider.
    ///
    /// # Security Warning
    ///
    /// Callers should use a fresh, unpredictable `state` for each authorization request and verify
    /// that this value matches the `state` parameter passed by the authorization server to the
    /// redirect URI. Doing so mitigates
    /// [Cross-Site Request Forgery](https://tools.ietf.org/html/rfc6749#section-10.12)
    ///  attacks.
    ///
    /// Similarly, callers should use a fresh, unpredictable `nonce` to help protect against ID
    /// token reuse and forgery.
    ///
    pub fn authorize_url<NF, RT, SF>(
        &self,
        authentication_flow: AuthenticationFlow<RT>,
        state_fn: SF,
        nonce_fn: NF,
    ) -> AuthorizationRequest<AD, P, RT>
    where
        NF: FnOnce() -> Nonce + 'static,
        RT: ResponseType,
        SF: FnOnce() -> CsrfToken + 'static,
    {
        AuthorizationRequest {
            inner: self.oauth2_client.authorize_url(state_fn),
            acr_values: Vec::new(),
            authentication_flow,
            claims_locales: Vec::new(),
            display: None,
            id_token_hint: None,
            login_hint: None,
            max_age: None,
            nonce: nonce_fn(),
            prompts: Vec::new(),
            ui_locales: Vec::new(),
        }
        .add_scope(Scope::new(OPENID_SCOPE.to_string()))
    }

    ///
    /// Creates a request builder for exchanging an authorization code for an access token.
    ///
    /// Acquires ownership of the `code` because authorization codes may only be used once to
    /// retrieve an access token from the authorization server.
    ///
    /// See https://tools.ietf.org/html/rfc6749#section-4.1.3
    ///
    pub fn exchange_code(&self, code: AuthorizationCode) -> CodeTokenRequest<TE, TR, TT> {
        self.oauth2_client.exchange_code(code)
    }

    ///
    /// Creates a request builder for exchanging a refresh token for an access token.
    ///
    /// See https://tools.ietf.org/html/rfc6749#section-6
    ///
    pub fn exchange_refresh_token<'a, 'b>(
        &'a self,
        refresh_token: &'b RefreshToken,
    ) -> RefreshTokenRequest<'b, TE, RR, TT>
    where
        'a: 'b,
    {
        self.refresh_oauth2_client
            .exchange_refresh_token(refresh_token)
    }

    ///
    /// Creates a request builder for info about the user associated with the given access token.
    ///
    /// This function requires that this [`Client`] be configured with a user info endpoint,
    /// which is an optional feature for OpenID Connect Providers to implement. If this `Client`
    /// does not know the provider's user info endpoint, it returns the [`NoUserInfoEndpoint`]
    /// error.
    ///
    /// To help protect against token substitution attacks, this function optionally allows clients
    /// to provide the subject identifier whose user info they expect to receive. If provided and
    /// the subject returned by the OpenID Connect Provider does not match, the
    /// [`UserInfoRequest::request`] or [`UserInfoRequest::request_async`] functions will return
    /// [`UserInfoError::ClaimsVerification`]. If set to `None`, any subject is accepted.
    ///
    pub fn user_info(
        &self,
        access_token: AccessToken,
        expected_subject: Option<SubjectIdentifier>,
    ) -> Result<UserInfoRequest<JE, JS, JT, JU, K>, NoUserInfoEndpoint> {
        Ok(UserInfoRequest {
            url: self
                .userinfo_endpoint
                .as_ref()
                .ok_or(NoUserInfoEndpoint)?
                .to_owned(),
            access_token,
            require_signed_response: false,
            signed_response_verifier: UserInfoVerifier::new(
                self.client_id.clone(),
                self.issuer.clone(),
                self.jwks.clone(),
                expected_subject,
            ),
        })
    }
}

///
/// A request to the authorization endpoint.
///
pub struct AuthorizationRequest<'a, AD, P, RT>
where
    AD: AuthDisplay,
    P: AuthPrompt,
    RT: ResponseType,
{
    inner: oauth2::AuthorizationRequest<'a>,
    acr_values: Vec<AuthenticationContextClass>,
    authentication_flow: AuthenticationFlow<RT>,
    claims_locales: Vec<LanguageTag>,
    display: Option<AD>,
    id_token_hint: Option<String>,
    login_hint: Option<LoginHint>,
    max_age: Option<Duration>,
    nonce: Nonce,
    prompts: Vec<P>,
    ui_locales: Vec<LanguageTag>,
}
impl<'a, AD, P, RT> AuthorizationRequest<'a, AD, P, RT>
where
    AD: AuthDisplay,
    P: AuthPrompt,
    RT: ResponseType,
{
    ///
    /// Appends a new scope to the authorization URL.
    ///
    pub fn add_scope(mut self, scope: Scope) -> Self {
        self.inner = self.inner.add_scope(scope);
        self
    }

    ///
    /// Appends an extra param to the authorization URL.
    ///
    /// This method allows extensions to be used without direct support from
    /// this crate. If `name` conflicts with a parameter managed by this crate, the
    /// behavior is undefined. In particular, do not set parameters defined by
    /// [RFC 6749](https://tools.ietf.org/html/rfc6749) or
    /// [RFC 7636](https://tools.ietf.org/html/rfc7636).
    ///
    /// # Security Warning
    ///
    /// Callers should follow the security recommendations for any OAuth2 extensions used with
    /// this function, which are beyond the scope of
    /// [RFC 6749](https://tools.ietf.org/html/rfc6749).
    ///
    pub fn add_extra_param<N, V>(mut self, name: N, value: V) -> Self
    where
        N: Into<Cow<'a, str>>,
        V: Into<Cow<'a, str>>,
    {
        self.inner = self.inner.add_extra_param(name, value);
        self
    }

    ///
    /// Enables the use of [Proof Key for Code Exchange](https://tools.ietf.org/html/rfc7636)
    /// (PKCE).
    ///
    /// PKCE is *highly recommended* for all public clients (i.e., those for which there
    /// is no client secret or for which the client secret is distributed with the client,
    /// such as in a native, mobile app, or browser app).
    ///
    pub fn set_pkce_challenge(mut self, pkce_code_challenge: PkceCodeChallenge) -> Self {
        self.inner = self.inner.set_pkce_challenge(pkce_code_challenge);
        self
    }

    ///
    /// Requests Authentication Context Class Reference values.
    ///
    /// ACR values should be added in order of preference. The Authentication Context Class
    /// satisfied by the authentication performed is accessible from the ID token via the
    /// [`IdTokenClaims::auth_context_ref`] method.
    ///
    pub fn add_auth_context_value(mut self, acr_value: AuthenticationContextClass) -> Self {
        self.acr_values.push(acr_value);
        self
    }

    ///
    /// Requests the preferred languages for claims returned by the OpenID Connect Provider.
    ///
    /// Languages should be added in order of preference.
    ///
    pub fn add_claims_locale(mut self, claims_locale: LanguageTag) -> Self {
        self.claims_locales.push(claims_locale);
        self
    }

    // TODO: support 'claims' parameter
    // https://openid.net/specs/openid-connect-core-1_0.html#ClaimsParameter

    ///
    /// Specifies how the OpenID Connect Provider displays the authentication and consent user
    /// interfaces to the end user.
    ///
    pub fn set_display(mut self, display: AD) -> Self {
        self.display = Some(display);
        self
    }

    ///
    /// Provides an ID token previously issued by this OpenID Connect Provider as a hint about
    /// the user's identity.
    ///
    /// This field should be set whenever [`core::CoreAuthPrompt::None`] is used (see
    /// [`AuthorizationRequest::add_prompt`]), it but may be provided for any authorization
    /// request.
    ///
    pub fn set_id_token_hint<AC, GC, JE, JS, JT>(
        mut self,
        id_token_hint: &'a IdToken<AC, GC, JE, JS, JT>,
    ) -> Self
    where
        AC: AdditionalClaims,
        GC: GenderClaim,
        JE: JweContentEncryptionAlgorithm<JT>,
        JS: JwsSigningAlgorithm<JT>,
        JT: JsonWebKeyType,
    {
        self.id_token_hint = Some(id_token_hint.to_string());
        self
    }

    ///
    /// Provides the OpenID Connect Provider with a hint about the user's identity.
    ///
    /// The nature of this hint is specific to each provider.
    ///
    pub fn set_login_hint(mut self, login_hint: LoginHint) -> Self {
        self.login_hint = Some(login_hint);
        self
    }

    ///
    /// Sets a maximum amount of time since the user has last authenticated with the OpenID
    /// Connect Provider.
    ///
    /// If more time has elapsed, the provider forces the user to re-authenticate.
    ///
    pub fn set_max_age(mut self, max_age: Duration) -> Self {
        self.max_age = Some(max_age);
        self
    }

    ///
    /// Specifies what level of authentication and consent prompts the OpenID Connect Provider
    /// should present to the user.
    ///
    pub fn add_prompt(mut self, prompt: P) -> Self {
        self.prompts.push(prompt);
        self
    }

    ///
    /// Requests the preferred languages for the user interface presented by the OpenID Connect
    /// Provider.
    ///
    /// Languages should be added in order of preference.
    ///
    pub fn add_ui_locale(mut self, ui_locale: LanguageTag) -> Self {
        self.ui_locales.push(ui_locale);
        self
    }

    ///
    /// Returns the full authorization URL and CSRF state for this authorization
    /// request.
    ///
    pub fn url(self) -> (Url, CsrfToken, Nonce) {
        let response_type = match self.authentication_flow {
            AuthenticationFlow::AuthorizationCode => core::CoreResponseType::Code.to_oauth2(),
            AuthenticationFlow::Implicit(include_token) => {
                if include_token {
                    OAuth2ResponseType::new(
                        vec![
                            core::CoreResponseType::IdToken,
                            core::CoreResponseType::Token,
                        ]
                        .iter()
                        .map(variant_name)
                        .collect::<Vec<_>>()
                        .join(" "),
                    )
                } else {
                    core::CoreResponseType::IdToken.to_oauth2()
                }
            }
            AuthenticationFlow::Hybrid(ref response_types) => OAuth2ResponseType::new(
                response_types
                    .iter()
                    .map(variant_name)
                    .collect::<Vec<_>>()
                    .join(" "),
            ),
        };
        let (mut inner, nonce) = (
            self.inner
                .set_response_type(&response_type)
                .add_extra_param("nonce", self.nonce.secret().clone()),
            self.nonce,
        );
        if !self.acr_values.is_empty() {
            inner = inner.add_extra_param("acr_values", join_vec(&self.acr_values));
        }
        if !self.claims_locales.is_empty() {
            inner = inner.add_extra_param("claims_locales", join_vec(&self.claims_locales));
        }
        if let Some(ref display) = self.display {
            inner = inner.add_extra_param("display", display.as_ref());
        }
        if let Some(ref id_token_hint) = self.id_token_hint {
            inner = inner.add_extra_param("id_token_hint", id_token_hint);
        }
        if let Some(ref login_hint) = self.login_hint {
            inner = inner.add_extra_param("login_hint", login_hint.secret());
        }
        if let Some(max_age) = self.max_age {
            inner = inner.add_extra_param("max_age", max_age.as_secs().to_string());
        }
        if !self.prompts.is_empty() {
            inner = inner.add_extra_param("prompt", join_vec(&self.prompts));
        }
        if !self.ui_locales.is_empty() {
            inner = inner.add_extra_param("ui_locales", join_vec(&self.ui_locales));
        }

        let (url, state) = inner.url();
        (url, state, nonce)
    }
}

///
/// Extends the base OAuth2 token response with an ID token.
///
pub trait TokenResponse<AC, GC, JE, JS, JT, TT>: OAuth2TokenResponse<TT>
where
    AC: AdditionalClaims,
    GC: GenderClaim,
    JE: JweContentEncryptionAlgorithm<JT>,
    JS: JwsSigningAlgorithm<JT>,
    JT: JsonWebKeyType,
    TT: TokenType,
{
    ///
    /// Returns the ID token provided by the token response.
    ///
    fn id_token(&self) -> &IdToken<AC, GC, JE, JS, JT>;
}

impl<AC, EF, GC, JE, JS, JT, TT> TokenResponse<AC, GC, JE, JS, JT, TT>
    for StandardTokenResponse<IdTokenFields<AC, EF, GC, JE, JS, JT>, TT>
where
    AC: AdditionalClaims,
    EF: ExtraTokenFields,
    GC: GenderClaim,
    JE: JweContentEncryptionAlgorithm<JT>,
    JS: JwsSigningAlgorithm<JT>,
    JT: JsonWebKeyType,
    TT: TokenType,
{
    fn id_token(&self) -> &IdToken<AC, GC, JE, JS, JT> {
        self.extra_fields().id_token()
    }
}

///
/// Extends the base OAuth2 token response with an optional ID token.
///
/// Unlike an initial token request, the ID token is an optional part of the response to a refresh
/// token request.
///
pub trait RefreshTokenResponse<AC, GC, JE, JS, JT, TT>: OAuth2TokenResponse<TT>
where
    AC: AdditionalClaims,
    GC: GenderClaim,
    JE: JweContentEncryptionAlgorithm<JT>,
    JS: JwsSigningAlgorithm<JT>,
    JT: JsonWebKeyType,
    TT: TokenType,
{
    ///
    /// Returns the optional ID token provided by the refresh token response.
    ///
    fn id_token(&self) -> Option<&IdToken<AC, GC, JE, JS, JT>>;
}

impl<AC, EF, GC, JE, JS, JT, TT> RefreshTokenResponse<AC, GC, JE, JS, JT, TT>
    for StandardTokenResponse<RefreshIdTokenFields<AC, EF, GC, JE, JS, JT>, TT>
where
    AC: AdditionalClaims,
    EF: ExtraTokenFields,
    GC: GenderClaim,
    JE: JweContentEncryptionAlgorithm<JT>,
    JS: JwsSigningAlgorithm<JT>,
    JT: JsonWebKeyType,
    TT: TokenType,
{
    fn id_token(&self) -> Option<&IdToken<AC, GC, JE, JS, JT>> {
        self.extra_fields().id_token()
    }
}

fn join_vec<T>(entries: &[T]) -> String
where
    T: AsRef<str>,
{
    entries
        .iter()
        .map(AsRef::as_ref)
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use oauth2::{AuthUrl, ClientId, ClientSecret, CsrfToken, RedirectUrl, Scope, TokenUrl};

    #[cfg(feature = "nightly")]
    use super::core::CoreAuthenticationFlow;
    use super::core::{CoreAuthDisplay, CoreAuthPrompt, CoreClient, CoreIdToken, CoreResponseType};
    use super::{
        AuthenticationContextClass, AuthenticationFlow, JsonWebKeySet, LanguageTag, LoginHint,
        Nonce,
    };
    use IssuerUrl;

    fn new_client() -> CoreClient {
        color_backtrace::install();
        CoreClient::new(
            ClientId::new("aaa".to_string()),
            Some(ClientSecret::new("bbb".to_string())),
            IssuerUrl::new("https://example".to_string()).unwrap(),
            AuthUrl::new("https://example/authorize".to_string()).unwrap(),
            Some(TokenUrl::new("https://example/token".to_string()).unwrap()),
            None,
            JsonWebKeySet::default(),
        )
    }

    #[test]
    fn test_authorize_url_minimal() {
        let client = new_client();

        let (authorize_url, _, _) = client
            .authorize_url(
                AuthenticationFlow::AuthorizationCode::<CoreResponseType>,
                || CsrfToken::new("CSRF123".to_string()),
                || Nonce::new("NONCE456".to_string()),
            )
            .url();

        assert_eq!(
            "https://example/authorize?response_type=code&client_id=aaa&\
             state=CSRF123&scope=openid&nonce=NONCE456",
            authorize_url.to_string()
        );
    }

    #[test]
    fn test_authorize_url_full() {
        let client = new_client()
            .set_redirect_uri(RedirectUrl::new("http://localhost:8888/".to_string()).unwrap());

        #[cfg(feature = "nightly")]
        let flow = CoreAuthenticationFlow::AuthorizationCode;
        #[cfg(not(feature = "nightly"))]
        let flow = AuthenticationFlow::AuthorizationCode::<CoreResponseType>;

        fn new_csrf() -> CsrfToken {
            CsrfToken::new("CSRF123".to_string())
        }
        fn new_nonce() -> Nonce {
            Nonce::new("NONCE456".to_string())
        }

        let (authorize_url, _, _) = client
            .authorize_url(flow.clone(), new_csrf, new_nonce)
            .add_scope(Scope::new("email".to_string()))
            .set_display(CoreAuthDisplay::Touch)
            .add_prompt(CoreAuthPrompt::Login)
            .add_prompt(CoreAuthPrompt::Consent)
            .set_max_age(Duration::from_secs(1800))
            .add_ui_locale(LanguageTag::new("fr-CA".to_string()))
            .add_ui_locale(LanguageTag::new("fr".to_string()))
            .add_ui_locale(LanguageTag::new("en".to_string()))
            .add_auth_context_value(AuthenticationContextClass::new(
                "urn:mace:incommon:iap:silver".to_string(),
            ))
            .url();
        assert_eq!(
            "https://example/authorize?response_type=code&client_id=aaa&\
             state=CSRF123&redirect_uri=http%3A%2F%2Flocalhost%3A8888%2F&scope=openid+email&\
             nonce=NONCE456&acr_values=urn%3Amace%3Aincommon%3Aiap%3Asilver&display=touch&\
             max_age=1800&prompt=login+consent&ui_locales=fr-CA+fr+en",
            authorize_url.to_string()
        );

        let serialized_jwt =
            "eyJhbGciOiJSUzI1NiJ9.eyJpc3MiOiJodHRwczovL2V4YW1wbGUuY29tIiwiYXVkIjpbIm15X2NsaWVudCJdL\
             CJleHAiOjE1NDQ5MzIxNDksImlhdCI6MTU0NDkyODU0OSwiYXV0aF90aW1lIjoxNTQ0OTI4NTQ4LCJub25jZSI\
             6InRoZV9ub25jZSIsImFjciI6InRoZV9hY3IiLCJzdWIiOiJzdWJqZWN0In0.gb5HuuyDMu-LvYvG-jJNIJPEZ\
             823qNwvgNjdAtW0HJpgwJWhJq0hOHUuZz6lvf8ud5xbg5GOo0Q37v3Ke08TvGu6E1USWjecZzp1aYVm9BiMvw5\
             EBRUrwAaOCG2XFjuOKUVfglSMJnRnoNqVVIWpCAr1ETjZzRIbkU3n5GQRguC5CwN5n45I3dtjoKuNGc2Ni-IMl\
             J2nRiCJOl2FtStdgs-doc-A9DHtO01x-5HCwytXvcE28Snur1JnqpUgmWrQ8gZMGuijKirgNnze2Dd5BsZRHZ2\
             CLGIwBsCnauBrJy_NNlQg4hUcSlGsuTa0dmZY7mCf4BN2WCpyOh0wgtkAgQ";
        let id_token = serde_json::from_value::<CoreIdToken>(serde_json::Value::String(
            serialized_jwt.to_string(),
        ))
        .unwrap();

        let (authorize_url, _, _) = client
            .authorize_url(flow, new_csrf, new_nonce)
            .add_scope(Scope::new("email".to_string()))
            .set_display(CoreAuthDisplay::Touch)
            .set_id_token_hint(&id_token)
            .set_login_hint(LoginHint::new("foo@bar.com".to_string()))
            .add_prompt(CoreAuthPrompt::Login)
            .add_prompt(CoreAuthPrompt::Consent)
            .set_max_age(Duration::from_secs(1800))
            .add_ui_locale(LanguageTag::new("fr-CA".to_string()))
            .add_ui_locale(LanguageTag::new("fr".to_string()))
            .add_ui_locale(LanguageTag::new("en".to_string()))
            .add_auth_context_value(AuthenticationContextClass::new(
                "urn:mace:incommon:iap:silver".to_string(),
            ))
            .add_extra_param("foo", "bar")
            .url();
        assert_eq!(
            format!(
                "https://example/authorize?response_type=code&client_id=aaa&state=CSRF123&\
                 redirect_uri=http%3A%2F%2Flocalhost%3A8888%2F&scope=openid+email&foo=bar&\
                 nonce=NONCE456&acr_values=urn%3Amace%3Aincommon%3Aiap%3Asilver&display=touch&\
                 id_token_hint={}&login_hint=foo%40bar.com&\
                 max_age=1800&prompt=login+consent&ui_locales=fr-CA+fr+en",
                serialized_jwt
            ),
            authorize_url.to_string()
        );
    }
}
