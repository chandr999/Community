FROM rust:1-bookworm AS build
WORKDIR /app
COPY Cargo.* ./
COPY src ./src
COPY migrations ./migrations
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=build /app/target/release/insta-clone /app/insta-clone
COPY static ./static
ENV BIND_ADDR=0.0.0.0:3000
EXPOSE 3000
CMD ["./insta-clone"]