# Custom Domains

Rise projects are accessible at their default URL (e.g., `https://my-app.rise.dev`). You can add custom domains to serve your application from your own domain names.

## Adding a Domain

```bash
rise domain add my-app myapp.example.com
```

With a `rise.toml` in your directory:

```bash
rise domain add myapp.example.com
```

## Listing Domains

```bash
rise domain list my-app
```

## Removing a Domain

```bash
rise domain remove my-app myapp.example.com
```

Aliases: `rise domain rm`, `rise domain del`

## DNS Configuration

Create a CNAME record pointing your domain to the Rise instance:

```
myapp.example.com.  CNAME  rise.example.com.
```

The exact target depends on your Rise installation — check with your platform team.

## Primary Domain

The first custom domain becomes the primary domain, used as the value of the `RISE_APP_URL` environment variable in your deployments. All domains (including the default URL) are included in `RISE_APP_URLS`.

## Domains in rise.toml

You can also define custom domains in `rise.toml`:

```toml
[project]
name = "my-app"
custom_domains = ["myapp.example.com", "api.example.com"]
```

Sync to the backend with:

```bash
rise project update --sync
```

## TLS

Rise handles TLS certificate provisioning for custom domains automatically.
