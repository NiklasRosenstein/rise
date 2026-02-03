# Authentication for Rise-Deployed Applications

Rise provides built-in authentication for your deployed applications using JWT tokens. When users authenticate to access your application, Rise issues a signed JWT token that your application can validate to identify the user.

## Overview

When a user logs into a Rise-deployed application:

1. Rise authenticates the user via OAuth2/OIDC (e.g., through Dex)
2. Rise issues an RS256-signed JWT token with user information
3. The JWT is stored in the `rise_jwt` cookie
4. Your application can access this cookie to identify the user

## The `rise_jwt` Cookie

The `rise_jwt` cookie contains a JWT token with the following structure:

### JWT Header
```json
{
  "alg": "RS256",
  "typ": "JWT",
  "kid": "<key-id>"
}
```

### JWT Claims (Example)
```json
{
  "sub": "CiQwOGE4Njg0Yi1kYjg4LTRiNzMtOTBhOS0zY2QxNjYxZjU0NjYSBWxvY2Fs",
  "email": "admin@example.com",
  "name": "admin",
  "groups": [],
  "iat": 1768858875,
  "exp": 1768945275,
  "iss": "http://localhost:3000",
  "aud": "http://test.rise.local:8080"
}
```

### Claim Descriptions

- **sub**: Unique user identifier from the identity provider (typically a base64-encoded UUID)
- **email**: User's email address
- **name**: User's display name (optional, included if available from IdP)
- **groups**: Array of Rise team names the user belongs to (empty array if user has no team memberships)
- **iat**: Issued at timestamp (Unix epoch seconds)
- **exp**: Expiration timestamp (Unix epoch seconds, default: 24 hours from issue time)
- **iss**: Issuer (Rise backend URL, e.g., `http://localhost:3000`)
- **aud**: Audience (your deployed application's URL, e.g., `http://test.rise.local:8080`)

**Note**: The JWT expiration time is configurable via the `jwt_expiry_seconds` server setting (default: 86400 seconds = 24 hours).

## Validating the JWT

Rise provides the public keys needed to validate JWTs through the standard OpenID Connect Discovery endpoint.

### OpenID Connect Discovery

Applications should use the OpenID Connect Discovery 1.0 specification to discover the JWKS endpoint:

1. **Fetch OpenID configuration** from `${RISE_ISSUER}/.well-known/openid-configuration`
2. **Extract `jwks_uri`** from the configuration response
3. **Fetch JWKS** from the `jwks_uri` endpoint
4. **Cache the JWKS** (recommended: 1 hour) to avoid excessive requests
5. **Use the JWKS** to validate JWT signatures

Example discovery response:
```json
{
  "issuer": "https://rise.example.com",
  "jwks_uri": "https://rise.example.com/api/v1/auth/jwks",
  "id_token_signing_alg_values_supported": ["RS256", "HS256"],
  "subject_types_supported": ["public"],
  "claims_supported": ["sub", "email", "name", "groups", "iat", "exp", "iss", "aud"]
}
```

### Environment Variables

Your deployed application automatically receives:

- **RISE_ISSUER**: Rise server URL (base URL for all Rise endpoints) and JWT issuer for validation (e.g., `http://localhost:3000`)
- **RISE_APP_URL**: Canonical URL where your app is accessible (primary custom domain or default project URL)
- **RISE_APP_URLS**: JSON array of all URLs your app is accessible at (primary ingress + custom domains), e.g., `["http://myapp.rise.local:8080", "https://myapp.example.com"]`
- **PORT**: The HTTP port your container should listen on (default: 8080)

### Example: Node.js/JavaScript

```javascript
const fetch = require('node-fetch');
const jwt = require('jsonwebtoken');
const jwkToPem = require('jwk-to-pem');

const RISE_ISSUER = process.env.RISE_ISSUER || 'http://localhost:3000';

// Cache for JWKS (refresh periodically in production)
let cachedJWKS = null;
let jwksFetchTime = null;
const JWKS_CACHE_DURATION = 3600000; // 1 hour in milliseconds

// Fetch JWKS from OpenID configuration discovery
async function fetchJWKS() {
  const configUrl = `${RISE_ISSUER}/.well-known/openid-configuration`;

  try {
    const configResponse = await fetch(configUrl);
    if (!configResponse.ok) {
      throw new Error(`Failed to fetch OpenID configuration: ${configResponse.status}`);
    }
    const config = await configResponse.json();

    const jwksResponse = await fetch(config.jwks_uri);
    if (!jwksResponse.ok) {
      throw new Error(`Failed to fetch JWKS: ${jwksResponse.status}`);
    }
    const jwks = await jwksResponse.json();

    return jwks;
  } catch (error) {
    console.error('Failed to fetch JWKS:', error);
    return null;
  }
}

// Get JWKS with caching
async function getJWKS() {
  const now = Date.now();

  // Return cached JWKS if valid
  if (cachedJWKS && jwksFetchTime && (now - jwksFetchTime) < JWKS_CACHE_DURATION) {
    return cachedJWKS;
  }

  // Fetch fresh JWKS
  cachedJWKS = await fetchJWKS();
  jwksFetchTime = now;

  return cachedJWKS;
}

// Convert JWKS to a key lookup function for jsonwebtoken
function getKey(header, callback) {
  getJWKS().then(jwks => {
    if (!jwks || !jwks.keys) {
      return callback(new Error('JWKS not available - JWT validation unavailable'));
    }

    const key = jwks.keys.find(k => k.kid === header.kid);
    if (!key) {
      return callback(new Error(`Key with kid "${header.kid}" not found in JWKS`));
    }

    try {
      const pem = jwkToPem(key);
      callback(null, pem);
    } catch (err) {
      callback(err);
    }
  }).catch(err => {
    callback(err);
  });
}

// Verify JWT from cookie
function verifyRiseJwt(req, res, next) {
  const token = req.cookies.rise_jwt;

  if (!token) {
    return res.status(401).send('No authentication token');
  }

  jwt.verify(token, getKey, {
    algorithms: ['RS256'],
    issuer: RISE_ISSUER,
    // Note: We skip audience validation here since the audience varies by deployment
    // In production, you should validate the audience matches your app's URL
  }, (err, decoded) => {
    if (err) {
      return res.status(401).send('Invalid token');
    }

    // Token is valid, attach user info to request
    req.user = {
      id: decoded.sub,
      email: decoded.email,
      name: decoded.name,
      groups: decoded.groups || []
    };

    next();
  });
}

// Use in Express middleware
app.use(cookieParser());
app.use(verifyRiseJwt);
```

### Example: Python/Flask

```python
import os
import json
import time
import requests
from jose import jwt, jwk
from flask import request, jsonify, g

RISE_ISSUER = os.environ.get('RISE_ISSUER', 'http://localhost:3000')

# Cache for JWKS (refresh periodically in production)
_jwks_cache = None
_jwks_cache_time = 0
JWKS_CACHE_DURATION = 3600  # 1 hour in seconds

def fetch_jwks():
    """Fetch JWKS from OpenID configuration discovery"""
    config_url = f'{RISE_ISSUER}/.well-known/openid-configuration'

    try:
        # Fetch OpenID configuration
        config_response = requests.get(config_url)
        config_response.raise_for_status()
        config = config_response.json()

        # Fetch JWKS from jwks_uri
        jwks_response = requests.get(config['jwks_uri'])
        jwks_response.raise_for_status()
        return jwks_response.json()
    except Exception as e:
        print(f'Failed to fetch JWKS: {e}')
        return None

def get_jwks():
    """Get JWKS with caching"""
    global _jwks_cache, _jwks_cache_time

    now = time.time()

    # Return cached JWKS if valid
    if _jwks_cache and (now - _jwks_cache_time) < JWKS_CACHE_DURATION:
        return _jwks_cache

    # Fetch fresh JWKS
    _jwks_cache = fetch_jwks()
    _jwks_cache_time = now

    return _jwks_cache

def verify_rise_jwt(token):
    """Verify and decode Rise JWT token"""
    try:
        # Fetch JWKS
        jwks = get_jwks()
        if not jwks or 'keys' not in jwks:
            raise ValueError('JWKS not available - JWT validation unavailable')

        # Decode header to get key ID
        headers = jwt.get_unverified_header(token)
        kid = headers['kid']

        # Find matching key in JWKS
        key = next((k for k in jwks['keys'] if k['kid'] == kid), None)
        if not key:
            raise ValueError(f'Key with kid "{kid}" not found in JWKS')

        # Convert JWK to PEM for verification
        public_key = jwk.construct(key)

        # Verify and decode token
        claims = jwt.decode(
            token,
            public_key.to_pem(),
            algorithms=['RS256'],
            issuer=RISE_ISSUER
            # Note: We skip audience validation here since the audience varies by deployment
            # In production, you should validate the audience matches your app's URL
        )

        return claims

    except Exception as e:
        raise ValueError(f'Token validation failed: {str(e)}')

@app.before_request
def authenticate():
    """Middleware to authenticate requests using Rise JWT"""
    token = request.cookies.get('rise_jwt')

    if not token:
        return jsonify({'error': 'No authentication token'}), 401

    try:
        claims = verify_rise_jwt(token)

        # Attach user info to request context
        g.user = {
            'id': claims['sub'],
            'email': claims['email'],
            'name': claims.get('name'),
            'groups': claims.get('groups', [])
        }
    except ValueError as e:
        return jsonify({'error': str(e)}), 401
```

### Example: Go

```go
package main

import (
    "context"
    "encoding/json"
    "fmt"
    "net/http"
    "os"
    "sync"
    "time"

    "github.com/golang-jwt/jwt/v5"
    "github.com/lestrrat-go/jwx/v2/jwk"
)

type RiseClaims struct {
    Sub    string   `json:"sub"`
    Email  string   `json:"email"`
    Name   string   `json:"name,omitempty"`
    Groups []string `json:"groups,omitempty"`
    jwt.RegisteredClaims
}

type OpenIDConfig struct {
    Issuer  string `json:"issuer"`
    JwksURI string `json:"jwks_uri"`
}

var (
    cachedJWKS      jwk.Set
    jwksCacheTime   time.Time
    jwksCacheMutex  sync.RWMutex
    jwksCacheDuration = 1 * time.Hour
)

func fetchJWKS() (jwk.Set, error) {
    riseIssuer := os.Getenv("RISE_ISSUER")
    if riseIssuer == "" {
        riseIssuer = "http://localhost:3000"
    }

    // Fetch OpenID configuration
    configURL := fmt.Sprintf("%s/.well-known/openid-configuration", riseIssuer)
    resp, err := http.Get(configURL)
    if err != nil {
        return nil, fmt.Errorf("failed to fetch OpenID configuration: %w", err)
    }
    defer resp.Body.Close()

    if resp.StatusCode != http.StatusOK {
        return nil, fmt.Errorf("failed to fetch OpenID configuration: status %d", resp.StatusCode)
    }

    var config OpenIDConfig
    if err := json.NewDecoder(resp.Body).Decode(&config); err != nil {
        return nil, fmt.Errorf("failed to decode OpenID configuration: %w", err)
    }

    // Fetch JWKS from jwks_uri
    jwksResp, err := http.Get(config.JwksURI)
    if err != nil {
        return nil, fmt.Errorf("failed to fetch JWKS: %w", err)
    }
    defer jwksResp.Body.Close()

    if jwksResp.StatusCode != http.StatusOK {
        return nil, fmt.Errorf("failed to fetch JWKS: status %d", jwksResp.StatusCode)
    }

    keySet, err := jwk.Parse(jwksResp.Body)
    if err != nil {
        return nil, fmt.Errorf("failed to parse JWKS: %w", err)
    }

    return keySet, nil
}

func getJWKS() (jwk.Set, error) {
    jwksCacheMutex.RLock()
    if cachedJWKS != nil && time.Since(jwksCacheTime) < jwksCacheDuration {
        defer jwksCacheMutex.RUnlock()
        return cachedJWKS, nil
    }
    jwksCacheMutex.RUnlock()

    jwksCacheMutex.Lock()
    defer jwksCacheMutex.Unlock()

    // Double-check after acquiring write lock
    if cachedJWKS != nil && time.Since(jwksCacheTime) < jwksCacheDuration {
        return cachedJWKS, nil
    }

    // Fetch fresh JWKS
    keySet, err := fetchJWKS()
    if err != nil {
        return nil, err
    }

    cachedJWKS = keySet
    jwksCacheTime = time.Now()

    return cachedJWKS, nil
}

func verifyRiseJWT(tokenString string) (*RiseClaims, error) {
    keySet, err := getJWKS()
    if err != nil {
        return nil, fmt.Errorf("failed to get JWKS: %w", err)
    }

    token, err := jwt.ParseWithClaims(tokenString, &RiseClaims{}, func(token *jwt.Token) (interface{}, error) {
        // Verify algorithm
        if _, ok := token.Method.(*jwt.SigningMethodRSA); !ok {
            return nil, fmt.Errorf("unexpected signing method: %v", token.Header["alg"])
        }

        // Get key ID from token header
        kid, ok := token.Header["kid"].(string)
        if !ok {
            return nil, fmt.Errorf("kid header missing")
        }

        // Find matching key in JWKS
        key, found := keySet.LookupKeyID(kid)
        if !found {
            return nil, fmt.Errorf("key not found in JWKS")
        }

        // Convert JWK to RSA public key
        var rawKey interface{}
        if err := key.Raw(&rawKey); err != nil {
            return nil, fmt.Errorf("failed to get raw key: %w", err)
        }

        return rawKey, nil
    })

    if err != nil {
        return nil, err
    }

    if claims, ok := token.Claims.(*RiseClaims); ok && token.Valid {
        return claims, nil
    }

    return nil, fmt.Errorf("invalid token")
}

func authMiddleware(next http.Handler) http.Handler {
    return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
        cookie, err := r.Cookie("rise_jwt")
        if err != nil {
            http.Error(w, "No authentication token", http.StatusUnauthorized)
            return
        }

        claims, err := verifyRiseJWT(cookie.Value)
        if err != nil {
            http.Error(w, fmt.Sprintf("Invalid token: %v", err), http.StatusUnauthorized)
            return
        }

        // Attach user info to context
        ctx := context.WithValue(r.Context(), "user", claims)
        next.ServeHTTP(w, r.WithContext(ctx))
    })
}
```

### Example: Rust

```rust
use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use std::env;

#[derive(Debug, Serialize, Deserialize)]
struct RiseClaims {
    sub: String,
    email: String,
    name: Option<String>,
    groups: Option<Vec<String>>,
    iat: u64,
    exp: u64,
    iss: String,
    aud: String,
}

#[derive(Debug, Deserialize)]
struct Jwks {
    keys: Vec<Jwk>,
}

#[derive(Debug, Deserialize)]
struct Jwk {
    kid: String,
    n: String,
    e: String,
}

fn get_jwks() -> Result<Jwks, Box<dyn std::error::Error>> {
    let jwks_json = env::var("RISE_JWKS").unwrap_or_else(|_| r#"{"keys":[]}"#.to_string());
    Ok(serde_json::from_str(&jwks_json)?)
}

fn verify_rise_jwt(token: &str) -> Result<RiseClaims, Box<dyn std::error::Error>> {
    let jwks = get_jwks()?;
    let header = decode_header(token)?;
    
    let kid = header.kid.ok_or("Missing kid in JWT header")?;
    
    // Find matching key in JWKS
    let jwk = jwks
        .keys
        .iter()
        .find(|k| k.kid == kid)
        .ok_or("Key not found in JWKS")?;
    
    // Decode the JWK components and create DecodingKey
    // Note: This requires the `rsa` crate and proper JWK-to-PEM conversion
    // For production use, consider using a library like `jsonwebtoken-jwks`
    
    // Simplified version - in production, properly convert JWK to DecodingKey
    let decoding_key = DecodingKey::from_rsa_components(&jwk.n, &jwk.e)?;
    
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(&[env::var("RISE_ISSUER").unwrap_or_else(|_| "https://rise.example.com".to_string())]);
    validation.set_audience(&[env::var("APP_URL").unwrap_or_else(|_| "https://myapp.apps.rise.example.com".to_string())]);
    
    let token_data = decode::<RiseClaims>(token, &decoding_key, &validation)?;
    Ok(token_data.claims)
}

async fn auth_middleware(
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Extract rise_jwt cookie
    let token = request
        .headers()
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .and_then(|cookies| {
            cookies.split(';').find_map(|cookie| {
                let cookie = cookie.trim();
                cookie.strip_prefix("rise_jwt=").map(|v| v.to_string())
            })
        })
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Verify JWT
    let claims = verify_rise_jwt(&token).map_err(|_| StatusCode::UNAUTHORIZED)?;
    
    // Add user info to request extensions
    // (You can access this in your handlers)
    
    Ok(next.run(request).await)
}
```

## Authorization Based on Groups

You can use the `groups` claim to implement team-based authorization:

```javascript
function requireTeam(teamName) {
  return (req, res, next) => {
    if (!req.user) {
      return res.status(401).send('Not authenticated');
    }
    
    if (!req.user.groups.includes(teamName)) {
      return res.status(403).send('Access denied - not a member of required team');
    }
    
    next();
  };
}

// Protect routes by team membership
app.get('/admin', requireTeam('admin'), (req, res) => {
  res.send('Admin panel');
});
```

## Best Practices

1. **Always Validate the JWT**: Don't trust the cookie contents without verification
2. **Check Expiration**: The JWT includes an `exp` claim - respect it
3. **Verify Audience**: Ensure the `aud` claim matches your application's URL
4. **Use HTTPS**: The `rise_jwt` cookie is marked as Secure in production
5. **Handle Missing Tokens**: Users may not be authenticated - handle gracefully
6. **Cache JWKS**: The JWKS rarely changes - cache the parsed keys in memory

## Troubleshooting

### Token Validation Fails

- **Check Algorithm**: Ensure you're using RS256, not HS256
- **Verify JWKS**: Confirm `RISE_JWKS` environment variable is set
- **Check Audience**: The `aud` claim must match your app's URL
- **Check Expiration**: Tokens expire after 1 hour by default

### No Cookie Present

- **Check Authentication**: User may not be logged in
- **Check Access Class**: Ensure your project has authentication enabled
- **Check Cookie Domain**: For custom domains, cookies may not be shared

### Groups Missing

- **Check IdP Configuration**: Groups come from your identity provider
- **Check Team Sync**: Ensure IdP group sync is enabled in Rise
- **Check Team Membership**: User must be a member of Rise teams

## Security Considerations

- The `rise_jwt` cookie is **HttpOnly** - JavaScript cannot access it
- The JWT is signed with RS256 - public keys in JWKS verify authenticity
- Tokens expire after 1 hour - users must re-authenticate periodically
- The `aud` claim ties tokens to specific applications

## Additional Resources

- [JWT.io](https://jwt.io/) - JWT debugger and documentation
- [JWKS Specification](https://datatracker.ietf.org/doc/html/rfc7517) - JSON Web Key Set standard
- [RS256 vs HS256](https://stackoverflow.com/questions/39239051/rs256-vs-hs256-whats-the-difference) - Understanding signature algorithms
