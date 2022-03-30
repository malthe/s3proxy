#!/bin/sh

owner=malthe
repo=s3proxy
tag=${1:?"missing arg 1 for 'tag'"}

docker build -t ghcr.io/$owner/$repo:$tag . \
       --label "org.opencontainers.image.source=https://github.com/$owner/$repo"
