import { User, Post, UserWithPosts } from "./types";

export function createUser(name: string, email: string): User {
  return {
    id: Math.floor(Math.random() * 10000),
    name,
    email,
    createdAt: new Date(),
  };
}

export function createPost(title: string, body: string, authorId: number): Post {
  return {
    id: Math.floor(Math.random() * 10000),
    title,
    body,
    authorId,
  };
}

export function attachPosts(user: User, posts: Post[]): UserWithPosts {
  return {
    ...user,
    posts: posts.filter((p) => p.authorId === user.id),
  };
}

export function formatUser(user: UserWithPosts): string {
  const postCount = user.posts.length;
  return `${user.name} (${user.email}) - ${postCount} post${postCount !== 1 ? "s" : ""}`;
}
