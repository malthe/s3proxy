S3 Proxy
========

This is a service implemented in Rust that proxies HTTP requests to an AWS S3-compatible endpoint.

### Configuration

The service is configured using environment variables:

| Name              | Description                                                     | Default              |
|-------------------|-----------------------------------------------------------------|----------------------|
| BIND_ADDRESS      |                                                                 | 0.0.0.0:&lt;PORT&gt; |
| PORT              |                                                                 | 8080                 |
| RULES_PATH        | The file must exist if a value is set here.                     | rules.txt            |
| S3_URL            | The AWS S3-compatible endpoint; this may include a path prefix. |                      |
| S3_ACCOUNT_KEY    |                                                                 |                      |
| S3_ACCOUNT_SECRET |                                                                 |                      |
| S3_REGION         |                                                                 |                      |
| CONNECT_TIMEOUT   |                                                                 | 1000                 |
| READ_TIMEOUT      |                                                                 | 5000                 |
| WRITE_TIMEOUT     |                                                                 | 5000                 |

### Rule-based authorization system

The service can be configured with a _rules file_ (defaults to "rules.txt").

If a rules file is provided, only requests matching at least one rule will be proxied.

The format of the rules file is plain text where each line is a rule. Each rule is a space-separated list of one or more tokens:

| Name        | Token                         | Match  |
|-------------|-------------------------------|--------|
| HTTP method | `GET`                         | Any    |
| Path prefix | `/images`                     | Any    |
| Header      | `x-secret-key=123`            | All    |
| Query       | `?list-type=2&prefix=images`  | Any    |

Example:
```
GET POST /images x-secret-key=123
```

Notes:

- Note that the header name is case-insensitive while the value must be an exact match.
- If the path prefix ends with "$" then the match must be exact (without the "$" suffix).

### Use cases

- The Linkerd service mesh supports [client identity](https://linkerd.io/2019/02/12/announcing-linkerd-2-2/) via the `l5d-client-id` header which can work as a simple authorization mechanism. Services can be set up with access to S3 with the exact privileges required without using IAM (which may not be desirable or supported) and/or complicated bucket policies.

