export interface User {
  id: number;
  name: string;
  email: string;
  createdAt: Date;
}

export interface Post {
  id: number;
  title: string;
  body: string;
  authorId: number;
}

export type UserWithPosts = User & {
  posts: Post[];
};
