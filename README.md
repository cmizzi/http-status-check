# Http status checker

![main workflow](https://github.com/cmizzi/http-status-check/workflows/Continuous%20integration/badge.svg)


> This is my first project in Rust. This is not intended
> to run in production.

This little binary project aims to crawl an entire website
in order to detect broken links.

## Usage

```bash
http-status-check <domain>
```

### Advanced usage

```
http-status-check --help
http-status-check --restrict-on-domain <domain>
http-status-check --restrict-on-domain --limit 100 <domain>
```

To always show passing URLs, you can also increment the
verbosity like that :

```
http-status-check --restrict-on-domain -v <domain>
```