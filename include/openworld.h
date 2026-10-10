/* SPDX-License-Identifier: Apache-2.0 */
#ifndef OPENWORLD_H
#define OPENWORLD_H

/* The phone and desktop shells call this library. They do not detect or compare.
 * Free every returned string with ow_string_free.
 *
 * ow_command takes {"argv":[...]} using the same arguments as the openworld program
 * for bundles, estimate, scan, and posters. ow_scan_request takes the scan object.
 */

char *ow_command(const char *request_json);
/* When argv contains --progress, each crop is one JSON line. The callback runs on the caller's thread. */
typedef void (*ow_progress_fn)(const char *json_line, void *user);
char *ow_command_progress(const char *request_json, ow_progress_fn progress, void *user);
char *ow_scan_request(const char *request_json);
void ow_string_free(char *ptr);

#endif
