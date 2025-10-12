ARG BUILDPLATFORM
FROM --platform=$BUILDPLATFORM rust:alpine AS build
WORKDIR /src
COPY . .

RUN USER=root apk add pkgconfig libc-dev ca-certificates libressl-dev
RUN cargo build --release

FROM scratch
WORKDIR /
COPY --from=build /src/target/release/SE3ClassroomBorrowingBackend ./serve
COPY --from=build /etc/ssl/certs /etc/ssl/certs

EXPOSE 3000

ENTRYPOINT ["./serve"]