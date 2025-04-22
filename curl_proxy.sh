#!/bin/bash

# proxy-curl.sh - Use your Rust proxy with curl

PROXY_HOST="localhost"
PROXY_PORT="8888"

if [ $# -eq 0 ]; then
    echo "Usage: $0 <URL> [curl options]"
    echo "Example: $0 http://example.com --verbose"
    exit 1
fi

URL=$1
shift

curl --proxy "http://$PROXY_HOST:$PROXY_PORT" "$URL" "$@"