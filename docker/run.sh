#!/bin/bash
set -ex

mkdir -p /srv/shyper-runner/{config,cargo/{git,registry}}

# Allow non-priveleged runners to create files in our volumes.
chmod 777 /srv/shyper-runner/{config,cargo/{git,registry}}

docker run -d --name shyper-runner --restart always \
    -v /srv/shyper-runner/config:/etc/gitlab-runner \
    -v /srv/shyper-runner/cargo/git:/home/shyper/.cargo/git \
    -v /srv/shyper-runner/cargo/registry:/home/shyper/.cargo/registry \
    -v /var/run/docker.sock:/var/run/docker.sock \
    shyper-runner:latest \
    run --user=shyper --working-directory=/home/shyper
