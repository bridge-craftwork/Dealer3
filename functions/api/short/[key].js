// `GET /api/short/<key>` — what a short link holds (#103). See
// `web/src/lib/shortLinks.js`.

import { readShortLink } from '../../../web/src/lib/shortLinks.js'

export const onRequestGet = ({ params, env }) => readShortLink(params.key, env.SHORT_LINKS)
