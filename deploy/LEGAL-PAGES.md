# Optional Meta policy pages

If Meta requires public Privacy Policy, Terms of Service, or Data Deletion URLs for your app, create and host those pages for your own deployment. This repository does not include legal advice or ready-made policy text.

For an existing HTTPS Nginx site, you can adapt `nginx-legal-locations.conf` to serve pages from `/var/www/ig-reply-bot-legal/`. Keep the paths and page contents under your control, run `nginx -t`, reload the proxy, and confirm each configured URL returns HTTP 200 before adding it to your Meta app settings.

Do not copy another operator's policy pages without reviewing them. The bot stores Instagram account, media, commenter, and comment IDs with delivery status in a local SQLite database until the database is removed; describe your own deployment's retention and contact details accurately.
