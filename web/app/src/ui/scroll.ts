/** The part of VS Code's view that scrolls under the strip. */
export const VIEW_SCROLL_ID = "view-scroll";

/** Goes back to the top of the page, or in VS Code's view to the top of the part under the
 * strip. */
export function toTop(): void {
  window.scrollTo(0, 0);
  document.getElementById(VIEW_SCROLL_ID)?.scrollTo(0, 0);
}
