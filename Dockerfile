FROM alpine:latest
RUN apk --no-cache add ca-certificates
COPY ./operator /bin/
EXPOSE 8080
ENTRYPOINT ["/bin/operator"]
