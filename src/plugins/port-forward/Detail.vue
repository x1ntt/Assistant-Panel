<script setup lang="ts">
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Network } from "lucide-vue-next";
</script>

<template>
  <div class="p-4 space-y-4">
    <div class="flex items-center gap-2 mb-4">
      <Network class="h-5 w-5" />
      <h2 class="text-lg font-semibold">Port Forward</h2>
    </div>
    <Card>
      <CardHeader>
        <CardTitle class="text-sm">Forward Rules</CardTitle>
      </CardHeader>
      <CardContent>
        <p class="text-sm text-muted-foreground">No forwarding rules configured.</p>
        <Button class="mt-4" size="sm">Add Rule</Button>
      </CardContent>
    </Card>
  </div>
</template>
