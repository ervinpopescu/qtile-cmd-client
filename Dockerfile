# syntax=docker/dockerfile:1

FROM myoung34/github-runner:ubuntu-noble
COPY /scripts/install-deps /
RUN chmod +x /install-deps
WORKDIR /home/runner
RUN /install-deps
COPY --from=myoung34/github-runner:ubuntu-noble token.sh entrypoint.sh app_token.sh /
RUN chmod +x /token.sh /entrypoint.sh /app_token.sh
COPY --from=myoung34/github-runner:ubuntu-noble /actions-runner /actions-runner
WORKDIR /actions-runner
ENTRYPOINT ["/entrypoint.sh"]
CMD ["./bin/Runner.Listener", "run", "--startuptype", "service"]
