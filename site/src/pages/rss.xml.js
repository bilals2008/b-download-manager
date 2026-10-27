import rss from "@astrojs/rss";
import { getCollection } from "astro:content";

export async function GET(context) {
  const docs = await getCollection("docs");

  return rss({
    title: "B Download Manager documentation",
    description:
      "Setup, day-to-day use and troubleshooting for B Download Manager.",
    site: context.site,
    items: docs.map((entry) => ({
      title: entry.data.title,
      description: entry.data.description,
      link: `/${entry.id}/`,
    })),
  });
}
