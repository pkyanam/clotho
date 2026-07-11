"use client";

import {
  ClerkProvider,
  OrganizationSwitcher,
  SignInButton,
  UserButton,
  useAuth,
} from "@clerk/nextjs";
import type { ReactNode } from "react";

/** True when a Clerk publishable key is configured (managed deploy). */
export function clerkEnabled(): boolean {
  return Boolean(
    process.env.NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY?.trim() ||
      process.env.CLERK_PUBLISHABLE_KEY?.trim(),
  );
}

/**
 * Wraps children with Clerk when publishable key is set.
 * Local/demo (`just demo`) leaves Clerk unset and uses bootstrap tokens.
 */
export function ClothoClerkProvider({ children }: { children: ReactNode }) {
  const key =
    process.env.NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY?.trim() ||
    process.env.CLERK_PUBLISHABLE_KEY?.trim();
  if (!key) {
    return <>{children}</>;
  }
  return <ClerkProvider publishableKey={key}>{children}</ClerkProvider>;
}

function ClerkAuthControlsInner() {
  const { isLoaded, isSignedIn } = useAuth();
  if (!isLoaded) {
    return null;
  }
  if (!isSignedIn) {
    return (
      <SignInButton mode="redirect">
        <button
          type="button"
          className="h-9 border border-kumo-hairline px-3 text-[0.8125rem] text-kumo-inactive transition-colors hover:border-kumo-contrast hover:text-kumo-default"
        >
          sign in
        </button>
      </SignInButton>
    );
  }
  return (
    <>
      <OrganizationSwitcher
        hidePersonal
        afterCreateOrganizationUrl="/"
        afterSelectOrganizationUrl="/"
        appearance={{
          elements: {
            rootBox: "flex items-center",
            organizationSwitcherTrigger:
              "h-9 border border-kumo-hairline px-2 text-[0.8125rem] text-kumo-inactive hover:text-kumo-default",
          },
        }}
      />
      <UserButton
        appearance={{
          elements: {
            avatarBox: "h-8 w-8",
          },
        }}
      />
    </>
  );
}

/** Header controls: org switcher + user button when Clerk is active. */
export function ClerkAuthControls() {
  const key =
    process.env.NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY?.trim() ||
    process.env.CLERK_PUBLISHABLE_KEY?.trim();
  if (!key) {
    return null;
  }
  return <ClerkAuthControlsInner />;
}
