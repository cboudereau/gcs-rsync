# GCP OAuth2

This crate is a refresh token rust backport of the golang gcp/oauth2

https://cs.opensource.google/go/x/oauth2/+/master:google/google.go?q=service_account&ss=go%2Fx%2Foauth2
https://developers.google.com/identity/protocols/oauth2/service-account#httprest

## Authorized User (dev account)

Setup your default account with [gcloud](https://cloud.google.com/sdk/gcloud/reference/auth/application-default)

## Service Account (prod account)

Setup a Service Account in the [console](https://cloud.google.com/docs/authentication/production#create_service_account)

## Usage

See usage in integration test : tests/token_integration_test.rs
