---
title: Images
source: https://developers.cloudflare.com/api/resources/images/
note: >
  Copy of the Cloudflare API reference index for Images, as supplied by the
  repository owner. Per-property "Link to this property" anchors and page
  chrome were stripped; content is otherwise unchanged. Request parameters are
  documented on each method page linked below.
---

# Images

## ImagesV1

| Method | HTTP | Notes |
|---|---|---|
| [List images](https://developers.cloudflare.com/api/resources/images/subresources/v1/methods/list) | `GET /accounts/{account_id}/images/v1` | Deprecated |
| [Image details](https://developers.cloudflare.com/api/resources/images/subresources/v1/methods/get) | `GET /accounts/{account_id}/images/v1/{image_id}` | |
| [Upload an image](https://developers.cloudflare.com/api/resources/images/subresources/v1/methods/create) | `POST /accounts/{account_id}/images/v1` | |
| [Update image](https://developers.cloudflare.com/api/resources/images/subresources/v1/methods/edit) | `PATCH /accounts/{account_id}/images/v1/{image_id}` | |
| [Delete image](https://developers.cloudflare.com/api/resources/images/subresources/v1/methods/delete) | `DELETE /accounts/{account_id}/images/v1/{image_id}` | |

### Models

**Image** `{ id, creator, filename, meta, requireSignedURLs, uploaded, variants }`

- `id`: optional string. Image unique identifier. maxLength 32
- `creator`: optional string. Can set the creator field with an internal user ID. maxLength 1024
- `filename`: optional string. Image file name. maxLength 255
- `meta`: optional unknown. User modifiable key-value store. Can be used for keeping references to another system of record for managing images. Metadata must not exceed 1024 bytes.
- `requireSignedURLs`: optional boolean. Indicates whether the image can be a accessed only using it's UID. If set to true, a signed token needs to be generated with a signing key to view the image.
- `uploaded`: optional string. When the media item was uploaded. format date-time
- `variants`: optional array of string. Object specifying available variants for an image.

**V1ListResponse** `{ images: optional array of Image }`

**V1DeleteResponse** = unknown or string

## ImagesV1Keys

| Method | HTTP |
|---|---|
| [List Signing Keys](https://developers.cloudflare.com/api/resources/images/subresources/v1/subresources/keys/methods/list) | `GET /accounts/{account_id}/images/v1/keys` |
| [Create a new Signing Key](https://developers.cloudflare.com/api/resources/images/subresources/v1/subresources/keys/methods/update) | `PUT /accounts/{account_id}/images/v1/keys/{signing_key_name}` |
| [Delete Signing Key](https://developers.cloudflare.com/api/resources/images/subresources/v1/subresources/keys/methods/delete) | `DELETE /accounts/{account_id}/images/v1/keys/{signing_key_name}` |

### Models

- **Key** `{ name: optional string (Key name), value: optional string (Key value) }`
- **KeyListResponse** `{ keys: optional array of Key }`
- **KeyUpdateResponse** `{ keys: optional array of Key }`
- **KeyDeleteResponse** `{ keys: optional array of Key }`

## ImagesV1Stats

| Method | HTTP |
|---|---|
| [Images usage statistics](https://developers.cloudflare.com/api/resources/images/subresources/v1/subresources/stats/methods/get) | `GET /accounts/{account_id}/images/v1/stats` |

### Models

- **Stat** `{ count: optional object { allowed: optional number (Cloudflare Images allowed usage), current: optional number (Cloudflare Images current usage) } }`

## ImagesV1Variants

| Method | HTTP |
|---|---|
| [List variants](https://developers.cloudflare.com/api/resources/images/subresources/v1/subresources/variants/methods/list) | `GET /accounts/{account_id}/images/v1/variants` |
| [Variant details](https://developers.cloudflare.com/api/resources/images/subresources/v1/subresources/variants/methods/get) | `GET /accounts/{account_id}/images/v1/variants/{variant_id}` |
| [Create a variant](https://developers.cloudflare.com/api/resources/images/subresources/v1/subresources/variants/methods/create) | `POST /accounts/{account_id}/images/v1/variants` |
| [Update a variant](https://developers.cloudflare.com/api/resources/images/subresources/v1/subresources/variants/methods/edit) | `PATCH /accounts/{account_id}/images/v1/variants/{variant_id}` |
| [Delete a variant](https://developers.cloudflare.com/api/resources/images/subresources/v1/subresources/variants/methods/delete) | `DELETE /accounts/{account_id}/images/v1/variants/{variant_id}` |

### Models

**Variant** `{ variants: optional object { hero: optional object { id, options, neverRequireSignedURLs } } }`

Variant definition `{ id, options, neverRequireSignedURLs }`:

- `id`: string. maxLength 99
- `options`: object `{ fit, height, metadata, width }`. Allows you to define image resizing sizes for different use cases.
  - `fit`: `"scale-down"` or `"contain"` or `"cover"` or `"crop"` or `"pad"`. The fit property describes how the width and height dimensions should be interpreted.
  - `height`: number. Maximum height in image pixels. minimum 1
  - `metadata`: `"keep"` or `"copyright"` or `"none"`. What EXIF data should be preserved in the output image.
  - `width`: number. Maximum width in image pixels. minimum 1
- `neverRequireSignedURLs`: optional boolean. Indicates whether the variant can access an image without a signature, regardless of image access control.

- **VariantGetResponse** `{ variant: optional variant definition }`
- **VariantCreateResponse** `{ variant: optional variant definition }`
- **VariantEditResponse** `{ variant: optional variant definition }`
- **VariantDeleteResponse** = unknown or string

## ImagesV1Blobs

| Method | HTTP |
|---|---|
| [Download image](https://developers.cloudflare.com/api/resources/images/subresources/v1/subresources/blobs/methods/get) | `GET /accounts/{account_id}/images/v1/{image_id}/blob` |

## ImagesV2

| Method | HTTP |
|---|---|
| [List images V2](https://developers.cloudflare.com/api/resources/images/subresources/v2/methods/list) | `GET /accounts/{account_id}/images/v2` |

### Models

**V2ListResponse** `{ continuation_token, images }`

- `continuation_token`: optional string. Continuation token to fetch next page. Passed as a query param when requesting List V2 api endpoint. maxLength 32
- `images`: optional array of Image

## ImagesV2Direct Uploads

| Method | HTTP |
|---|---|
| [Create authenticated direct upload URL V2](https://developers.cloudflare.com/api/resources/images/subresources/v2/subresources/direct_uploads/methods/create) | `POST /accounts/{account_id}/images/v2/direct_upload` |

### Models

**DirectUploadCreateResponse** `{ id, uploadURL }`

- `id`: optional string. Image unique identifier. maxLength 32
- `uploadURL`: optional string. The URL the unauthenticated upload can be performed to using a single HTTP POST (multipart/form-data) request.
