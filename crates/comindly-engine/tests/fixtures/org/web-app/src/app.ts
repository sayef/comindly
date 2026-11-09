import { Button, Theme } from "@acme/ui-kit";
import * as kit from "@acme/ui-kit";

export class DarkTheme implements Theme {
  name = "dark";
}

export function render(): string {
  const f = kit.createFormatter();
  return Button("ok") + kit.formatDate(new Date()) + f.format(new Date());
}
