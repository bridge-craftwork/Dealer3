// `POST /api/short` — make a short link (#103). The logic, and the reasons for
// it, are in `web/src/lib/shortLinks.js`; this only hands it the KV binding.

import { createShortLink } from '../../../web/src/lib/shortLinks.js'

export const onRequestPost = ({ request, env }) => createShortLink(request, env.SHORT_LINKS)
