The product requirement changed again. Two browser tabs may connect to
the same generation id. Both must observe the same generation and the
same ordered token sequence; no second generation may start.
Keep the disconnect and resume behavior from the previous requirement.

Add `DELETE /api/generations/:id`. It returns 202, stops the generator,
and emits one `cancelled` event to every connected tab. Update the browser
client so a shared id can be opened and cancellation is visible in every
tab. Run all tests and finish the implementation.
