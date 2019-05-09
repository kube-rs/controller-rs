FROM alpine:latest
RUN apk --no-cache add ca-certificates
COPY ./controller /bin/
EXPOSE 8080
ENTRYPOINT ["/bin/controller"]
