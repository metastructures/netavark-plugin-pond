# Security Policy

## Supported Versions

Only the latest release receives security fixes. No backports.

## Reporting a Vulnerability

Do not open a public issue for security vulnerabilities. Use GitHub's private
advisory mechanism instead:

**https://github.com/metastructures/netavark-plugin-pond/security/advisories/new**

Expect acknowledgement within 7 days. The standard disclosure timeline is
90 days from acknowledgement. Vulnerabilities will be disclosed publicly after
that window regardless of patch status.

## Scope

This plugin runs with elevated privileges and directly manipulates network
namespaces and OVS port configuration. The following are in scope:

- Input handling from the netavark daemon (network config parsing, option validation)
- Privilege boundary violations or namespace escapes
- Incorrect teardown leaving persistent network state

Dependency advisories that do not affect code paths exercised by this plugin
are out of scope. If filing such a report, demonstrate a realistic attack vector;
reports that amount to "a transitive dependency has an advisory" without a
concrete path will be triaged accordingly.
