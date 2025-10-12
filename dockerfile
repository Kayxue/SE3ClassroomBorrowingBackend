ARG BUILDPLATFORM
ARG PASSWORD_HASHING_SECRET
ENV PASSWORD_HASHING_SECRET=${PASSWORD_HASHING_SECRET}
FROM --platform=$BUILDPLATFORM rust:alpine AS build
WORKDIR /src
COPY . .

RUN USER=root apk add pkgconfig libc-dev
RUN cargo build --release

FROM scratch
WORKDIR /
COPY --from=build /src/target/release/SE3ClassroomBorrowingBackend ./serve

EXPOSE 3000

ENTRYPOINT ["./serve"]