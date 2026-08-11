import { desc, eq } from "drizzle-orm";
import { drizzle } from "drizzle-orm/d1";
import * as schema from "../db/schema";

export type RobotBuild = Record<string, unknown>;

export class RobotService {
  private readonly db;

  constructor(database: D1Database) {
    this.db = drizzle(database, { schema });
  }

  async listForUser(userId: string) {
    const robots = await this.db
      .select()
      .from(schema.robots)
      .where(eq(schema.robots.userId, userId))
      .orderBy(desc(schema.robots.updatedAt));
    return robots.map(RobotService.toDto);
  }

  async create(userId: string, input: { name: string; buildData: RobotBuild }) {
    const now = new Date();
    const robot = {
      id: crypto.randomUUID(),
      userId,
      name: input.name,
      buildData: JSON.stringify(input.buildData),
      createdAt: now,
      updatedAt: now,
    };
    await this.db.insert(schema.robots).values(robot);
    return RobotService.toDto(robot);
  }

  static toDto(robot: typeof schema.robots.$inferSelect) {
    return {
      id: robot.id,
      name: robot.name,
      buildData: JSON.parse(robot.buildData) as RobotBuild,
      createdAt: robot.createdAt,
      updatedAt: robot.updatedAt,
    };
  }
}
