<script setup lang="ts">
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Globe } from "lucide-vue-next";
</script>

<template>
  <div class="p-4 space-y-4">
    <div class="flex items-center gap-2 mb-4">
      <Globe class="h-5 w-5" />
      <h2 class="text-lg font-semibold">API Dashboard</h2>
    </div>
    <Card>
      <CardHeader>
        <CardTitle class="text-sm">API Endpoints</CardTitle>
      </CardHeader>
      <CardContent>
        <p class="text-sm text-muted-foreground">No API endpoints configured. Add an endpoint to monitor.</p>
        <Button class="mt-4" size="sm">Add Endpoint</Button>
      </CardContent>
    </Card>
  </div>
</template>
