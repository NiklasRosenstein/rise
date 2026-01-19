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

### JWT Claims
```json
{
  "sub": "<user-id>",
  "email": "user@example.com",
  "name": "User Name",
  "groups": ["team1", "team2"],
  "iat": 1234567890,
  "exp": 1234571490,
  "iss": "<rise-backend-url>",
  "aud": "<your-app-url>"
}
```

### Claim Descriptions

- **sub**: Unique user identifier from the identity provider
- **email**: User's email address
- **name**: User's display name (optional)
- **groups**: List of Rise teams the user belongs to
- **iat**: Issued at timestamp (Unix epoch)
- **exp**: Expiration timestamp (Unix epoch)
- **iss**: Issuer (Rise backend URL)
- **aud**: Audience (your application's URL)

## Validating the JWT

Rise provides the public keys needed to validate JWTs through the `RISE_JWKS` environment variable.

### Environment Variables

Your deployed application automatically receives:

- **RISE_JWKS**: JSON Web Key Set (JWKS) containing RS256 public keys for JWT verification
- **PORT**: The port your application should listen on

### Example: Node.js/JavaScript

```javascript
const jwksClient = require('jwks-rsa');
const jwt = require('jsonwebtoken');

// Parse JWKS from environment variable
const jwks = JSON.parse(process.env.RISE_JWKS || '{"keys":[]}');

// Create JWKS client
const client = jwksClient({
  jwksUri: null, // Not needed - we have the keys directly
  cache: true,
  rateLimit: true
});

// Or use the keys directly
function getKey(header, callback) {
  const key = jwks.keys.find(k => k.kid === header.kid);
  if (!key) {
    return callback(new Error('Key not found'));
  }
  // Convert JWK to PEM format or use directly with library
  callback(null, key);
}

// Verify JWT from cookie
function verifyRiseJwt(req, res, next) {
  const token = req.cookies.rise_jwt;
  
  if (!token) {
    return res.status(401).send('No authentication token');
  }

  jwt.verify(token, getKey, {
    algorithms: ['RS256'],
    issuer: process.env.RISE_ISSUER || 'https://rise.example.com',
    audience: process.env.APP_URL || 'https://myapp.apps.rise.example.com'
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
from jose import jwt, jwk
from jose.utils import base64url_decode
from flask import request, jsonify

# Load JWKS from environment
jwks = json.loads(os.environ.get('RISE_JWKS', '{"keys":[]}'))

def verify_rise_jwt(token):
    """Verify and decode Rise JWT token"""
    try:
        # Decode header to get key ID
        headers = jwt.get_unverified_header(token)
        kid = headers['kid']
        
        # Find matching key in JWKS
        key = next((k for k in jwks['keys'] if k['kid'] == kid), None)
        if not key:
            raise ValueError('Key not found in JWKS')
        
        # Convert JWK to PEM for verification
        public_key = jwk.construct(key)
        
        # Verify and decode token
        claims = jwt.decode(
            token,
            public_key.to_pem(),
            algorithms=['RS256'],
            issuer=os.environ.get('RISE_ISSUER', 'https://rise.example.com'),
            audience=os.environ.get('APP_URL', 'https://myapp.apps.rise.example.com')
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
    "encoding/json"
    "fmt"
    "net/http"
    "os"
    
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

func getJWKS() (jwk.Set, error) {
    jwksJSON := os.Getenv("RISE_JWKS")
    if jwksJSON == "" {
        jwksJSON = `{"keys":[]}`
    }
    
    return jwk.Parse([]byte(jwksJSON))
}

func verifyRiseJWT(tokenString string) (*RiseClaims, error) {
    keySet, err := getJWKS()
    if err != nil {
        return nil, fmt.Errorf("failed to parse JWKS: %w", err)
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
