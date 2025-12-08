# Hello World Example

A simple Node.js Express application for testing the `rise deployment create` command.

## Local Development

```bash
# Install dependencies
npm install

# Run locally
npm start

# Test
curl http://localhost:8080
```

## Deploy with Rise

```bash
# Login to Rise
rise login --email test@example.com --password test1234

# Create a project
rise project create hello-world --visibility public

# Deploy the application
rise deployment create hello-world example/hello-world
```

The application will be built using Cloud Native Buildpacks and pushed to the configured registry.

## Endpoints

- `GET /` - Returns a JSON hello world message
- `GET /health` - Health check endpoint

## Environment Variables

- `PORT` - Port to listen on (default: 8080)
