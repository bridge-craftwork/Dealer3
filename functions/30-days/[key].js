// `GET /30-days/<key>` — the short link people are sent (#103). It hands over
// to the page, which fetches what the key holds. See `web/src/lib/shortLinks.js`.
//
// The directory name is the promise the link makes, and `SHORT_LINK_PATH` in
// `web/src/lib/shortKey.js` must say the same; a test holds the two together.

import { redirectShortLink } from '../../web/src/lib/shortLinks.js'

export const onRequestGet = ({ params }) => redirectShortLink(params.key)
