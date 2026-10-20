import { useState } from "react";
import { useT } from "../../lib/i18n";
import { cmd, errorMessage } from "../../lib/ipc/commands";
import type { ServerInfo } from "../../lib/subsonic/types";
import { useUiStore } from "../../stores/uiStore";
import { Button } from "../../components/ui/Button";
import { Input } from "../../components/ui/Input";
import { Logo } from "../../components/ui/Logo";
import { Spinner } from "../../components/ui/Spinner";

type TestState =
  | { kind: "idle" }
  | { kind: "testing" }
  | { kind: "ok"; info: ServerInfo }
  | { kind: "error"; message: string };

export function SetupView() {
  const t = useT();
  const setSession = useUiStore((s) => s.setSession);
  const [serverUrl, setServerUrl] = useState("");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [test, setTest] = useState<TestState>({ kind: "idle" });
  const [saving, setSaving] = useState(false);

  const canSubmit = serverUrl.trim() !== "" && username.trim() !== "" && password !== "";

  async function testConnection() {
    setTest({ kind: "testing" });
    try {
      const info = await cmd("auth_test", { serverUrl, username, password });
      setTest({ kind: "ok", info });
    } catch (e) {
      setTest({ kind: "error", message: errorMessage(e) });
    }
  }

  async function connect(e: React.FormEvent) {
    e.preventDefault();
    if (!canSubmit || saving) return;
    setSaving(true);
    try {
      await cmd("auth_save", { serverUrl, username, password });
      setSession("ready");
    } catch (err) {
      setTest({ kind: "error", message: errorMessage(err) });
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="flex h-full items-center justify-center">
      <form onSubmit={connect} className="w-96 rounded-xl border border-edge bg-surface p-8">
        <div className="mb-6 flex items-center gap-3">
          <Logo className="size-10" />
          <div>
            <h1 className="text-xl font-semibold">Reverb</h1>
            <p className="text-xs text-dim">{t("setup.subtitle")}</p>
          </div>
        </div>

        <label className="mb-1 block text-xs font-medium text-dim">{t("setup.serverUrl")}</label>
        <Input
          value={serverUrl}
          onChange={(e) => setServerUrl(e.target.value)}
          placeholder="https://music.example.com"
          autoFocus
          spellCheck={false}
          className="mb-3"
        />
        <label className="mb-1 block text-xs font-medium text-dim">{t("setup.username")}</label>
        <Input
          value={username}
          onChange={(e) => setUsername(e.target.value)}
          autoComplete="username"
          spellCheck={false}
          className="mb-3"
        />
        <label className="mb-1 block text-xs font-medium text-dim">{t("setup.password")}</label>
        <Input
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          type="password"
          autoComplete="current-password"
          className="mb-4"
        />

        <div className="mb-4 min-h-6 text-xs">
          {test.kind === "testing" && (
            <span className="flex items-center gap-2 text-dim">
              <Spinner className="size-3.5" /> {t("setup.testing")}
            </span>
          )}
          {test.kind === "ok" && (
            <span className="text-green-400">
              {t("setup.connected", {
                server: `${test.info.serverType ?? "Subsonic"} ${test.info.serverVersion ?? ""}`.trim(),
              })}
            </span>
          )}
          {test.kind === "error" && <span className="text-danger">{test.message}</span>}
        </div>

        <div className="flex gap-2">
          <Button
            type="button"
            variant="outline"
            onClick={testConnection}
            disabled={!canSubmit || test.kind === "testing"}
            className="flex-1"
          >
            {t("setup.testConnection")}
          </Button>
          <Button type="submit" disabled={!canSubmit || saving} className="flex-1">
            {saving ? t("setup.connecting") : t("setup.connect")}
          </Button>
        </div>
      </form>
    </div>
  );
}
