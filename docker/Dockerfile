FROM gitlab/gitlab-runner:ubuntu

ENV DEBIAN_FRONTEND=noninteractive
ARG MIRROR=mirrors.tuna.tsinghua.edu.cn

RUN sed -i "s/archive.ubuntu.com/${MIRROR}/g" /etc/apt/sources.list; \
    sed -i "s/security.ubuntu.com/${MIRROR}/g" /etc/apt/sources.list; \
    apt-get update && apt-get -y upgrade && \
    apt-get install -y curl build-essential gcc-aarch64-linux-gnu clang git && \
    apt-get clean

ARG RUNNER_USER=shyper
ARG RUNNER_HOME=/home/${RUNNER_USER}

RUN useradd -m ${RUNNER_USER}
COPY rustup.sh ${RUNNER_HOME}/
RUN chown -R ${RUNNER_USER}:${RUNNER_USER} ${RUNNER_HOME}

USER ${RUNNER_USER}
ENV HOME=${RUNNER_HOME}
WORKDIR ${RUNNER_HOME}

# ENV HTTP_PROXY=http://172.17.0.1:8080
# ENV HTTPS_PROXY="$HTTP_PROXY" ALL_PROXY="$HTTP_PROXY" \
#     http_proxy="$HTTP_PROXY" https_proxy="$HTTP_PROXY" all_proxy="$HTTP_PROXY"

RUN ~/rustup.sh -y
ENV PATH="${RUNNER_HOME}/.cargo/bin:$PATH"

# ENV HTTP_PROXY= HTTPS_PROXY= ALL_PROXY= http_proxy= https_proxy= all_proxy=
