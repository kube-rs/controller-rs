FROM alpine:3.9
RUN apk --no-cache add ca-certificates && \
    adduser usr -Du 1000 -h /app
COPY ./controller /app/
RUN chown usr:usr /app/controller
EXPOSE 8080
USER usr
ENTRYPOINT ["/app/controller"]
