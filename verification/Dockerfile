FROM rust:1.90-bookworm
RUN sed -i s,http://deb.debian.org,https://deb.debian.org,g /etc/apt/sources.list.d/debian.sources && \
    apt-get update -qq && apt-get install -y -qq --no-install-recommends ffmpeg
WORKDIR /work
