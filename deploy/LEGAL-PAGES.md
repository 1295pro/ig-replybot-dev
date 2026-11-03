# Meta policy and data-deletion pages

Each operator uses their own Meta app and must provide policy links that accurately describe that operator's service. Meta's App Dashboard has fields for a Privacy Policy URL, Terms of Service URL, and data-deletion instructions. Meta may require these links depending on the app and permissions being published. Keep the pages public, reachable over HTTPS, and specific to your own data handling and contact process. This repository does not include legal advice or ready-made policy text.

For an existing HTTPS Nginx site, you can adapt `nginx-legal-locations.conf` to serve pages from `/var/www/ig-reply-bot-legal/`. Keep the paths and page contents under your control, run `nginx -t`, reload the proxy, and confirm each configured URL returns HTTP 200 before adding it to your Meta app settings.

Do not copy another operator's policy pages without reviewing them. The bot stores Instagram account, media, commenter, and comment IDs with delivery status in a local SQLite database until the database is removed. Describe your own deployment's retention, hosting, contact details, and deletion process accurately. A working policy page does not by itself grant Advanced Access to Meta permissions or guarantee App Review approval.
