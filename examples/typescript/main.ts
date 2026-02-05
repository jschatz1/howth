import { createUser, createPost, attachPosts, formatUser } from "./utils";
import { User, Post } from "./types";

// Create some sample data
const user: User = createUser("Alice", "alice@example.com");

const posts: Post[] = [
  createPost("Hello World", "My first post!", user.id),
  createPost("TypeScript is Great", "I love type safety.", user.id),
  createPost("Howth is Fast", "Direct TS execution is amazing.", user.id),
];

// Attach posts to user
const userWithPosts = attachPosts(user, posts);

// Display results
console.log("=== TypeScript Example ===\n");
console.log("User:", formatUser(userWithPosts));
console.log("\nPosts:");
userWithPosts.posts.forEach((post, i) => {
  console.log(`  ${i + 1}. ${post.title}`);
});

console.log("\n=== Type Safety Demo ===");
console.log(`User ID type: ${typeof user.id}`);
console.log(`Created at: ${user.createdAt.toISOString()}`);
