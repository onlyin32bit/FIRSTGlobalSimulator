import { Hono } from "hono";
import { isResponse, parseJson, robotSchema } from "../lib/validation";
import { requireUser } from "../middleware";
import { jsonError, jsonSuccess } from "../responses";
import { RobotService } from "../services/robot-service";
import type { Bindings } from "../types";

const app = new Hono<{ Bindings: Bindings }>();

app.get("/", async (c) => {
  const session = await requireUser(c);
  if (!session) return jsonError(c, 401, "AUTH_FAILED", "Sign in is required.");
  return jsonSuccess(c, {
    robots: await new RobotService(c.env.DB).listForUser(session.user.id),
  });
});

app.post("/", async (c) => {
  const session = await requireUser(c);
  if (!session) return jsonError(c, 401, "AUTH_FAILED", "Sign in is required.");

  const body = await parseJson(c, robotSchema);
  if (isResponse(body)) return body;

  const robot = await new RobotService(c.env.DB).create(session.user.id, body);
  return jsonSuccess(c, { robot }, 201);
});

export default app;
