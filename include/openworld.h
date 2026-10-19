/* SPDX-License-Identifier: Apache-2.0 */
#ifndef OPENWORLD_H
#define OPENWORLD_H

#include <stdint.h>

/* The phone and desktop shells call this library. They do not detect, track, or compare.
 * Every request is JSON with "v": 1 and an "op". Every answer is JSON. A refusal has
 * "ok": false and a "headline" line {code, text, args} to show. Shells translate by code.
 * Free every returned string with ow_string_free. No call panics or aborts on bad input.
 */

/* Events are JSON lines: {"event":"progress"|"face"|"track", ...}. They arrive on the
 * thread that made the call that produced them. */
typedef void (*ow_event_fn)(const char *json, void *user);

/* ops: bundles, estimate, scan (desktop: the library decodes the file), posters_check,
 * posters_demo, posters_update, leave, delete, sweep, copy, version. */
char *ow_call(const char *request_json, ow_event_fn on_event, void *user);

/* A scan of frames the shell decodes (AVFoundation, MediaCodec). The request is a scan
 * request with "media": {width, height, frames, fps, duration_sec, video, container_unix}.
 * Returns NULL on refusal and writes the refusal JSON to *refusal (free it). */
typedef struct OwJob ow_job;
ow_job *ow_job_start(const char *request_json, ow_event_fn on_event, void *user, char **refusal);

/* Frames go in order, every decoded frame. Before converting frame `index`, ask
 * ow_job_wants; on 0, call ow_job_skip instead of pushing it. */
int ow_job_wants(ow_job *job, uint64_t index);
int ow_job_skip(ow_job *job, uint64_t index);

/* layout: 0 RGBA, 1 BGRA, 2 RGB. rotation: 0, 90, 180, 270 degrees clockwise to show the
 * picture upright. Returns 0 when the scan has stopped; finish it then. */
int ow_job_push_pixels(ow_job *job, uint64_t index, const uint8_t *pixels, uint32_t width, uint32_t height,
                       uint32_t stride, uint32_t layout, uint32_t rotation);
/* 8-bit 4:2:0. uv_step: 1 planar (I420), 2 interleaved (NV12/NV21: pass the U and V pointers). */
int ow_job_push_yuv420(ow_job *job, uint64_t index, const uint8_t *y, uint32_t y_stride, const uint8_t *u,
                       const uint8_t *v, uint32_t uv_stride, uint32_t uv_step, uint32_t width, uint32_t height,
                       int full_range, int bt709, uint32_t rotation);

/* Stop a job from any thread. reason: "cancel", "heat", "battery", "background".
 * The report is then Incomplete and says why. */
typedef struct OwStop ow_stop_token;
ow_stop_token *ow_job_stop_token(ow_job *job);
void ow_stop(ow_stop_token *token, const char *reason);
void ow_stop_free(ow_stop_token *token);

/* end_json: {"clean": true} when the decoder reached the end without an error.
 * Returns the report and frees the job. */
char *ow_job_finish(ow_job *job, const char *end_json);
/* Abandon a job without a report. */
void ow_job_free(ow_job *job);

void ow_string_free(char *ptr);

#endif
