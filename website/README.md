# Download-site hosting

The download site is static HTML, CSS and browser JavaScript. Both hosting jobs package the same six public files; app binaries and updater manifests remain on GitHub Releases.

- GitHub Pages: `.github/workflows/pages.yml`.
- GitLab Pages trial: `.gitlab-ci.yml`, job `deploy-download-site`, publishing `.gitlab-pages` with Node 22. It runs for site/config changes on the default branch, or an explicitly started web/API pipeline.
- Local contract tests: `node --test website/tests/*.test.mjs`.
- Browser checks: serve the packaged site, then run `FNDR_SITE_URL=http://127.0.0.1:5193/ node website/tests/browser-check.mjs` with the repo's Playwright dependency and Chrome installed.

## GitLab trial on 2026-11-17

The trial asks whether `capstone.cs.utah.edu` can host the existing site without changing its release source. GitLab 19.3.9 accepted the configuration with no errors or warnings. All 12 site tests and the real Chrome acceptance script passed against the locally staged artifact.

[Pipeline 29433](https://capstone.cs.utah.edu/fndr/fndr/-/pipelines/29433) created [job 93195](https://capstone.cs.utah.edu/fndr/fndr/-/jobs/93195), but the job remained pending with no assigned runner. The project's available-runner list was empty. Pages access was set to public while repository visibility remained private; no site deployment or live Pages URL was created.

To continue, attach a runner that accepts untagged jobs and can run the Node 22 image, then retry the pipeline. Confirm the instance's Pages domain with its administrator: the public [instance configuration](https://capstone.cs.utah.edu/help/instance_configuration) currently lists `example.com`. Do not infer a usable site address from that placeholder. The trial keeps the existing GitHub canonical URL until a working GitLab URL is verified.

The trial is isolated on `feat/gitlab-pages`. Keep the working GitHub Pages deployment in place until GitLab returns a deployed URL and an unauthenticated browser can load its page, assets and download links.
